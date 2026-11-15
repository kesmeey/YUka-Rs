//! YKS001 expressions, conditionals, and script frames with explicit host boundaries.
//! Unimplemented expressions and control flow return errors without being skipped.
use crate::Result;
use crate::script::{Element, InstructionKind, Script, Text};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Integer(i32),
    String(Text),
    Label { name: String, link: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Slot {
    IntegerVariable { bank: String, id: i32 },
    StringVariable { bank: String, id: i32 },
    Local { id: u32 },
    DataWord { offset: u32 },
    SpecialString { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Target {
        word_offset: u32,
        slot: Slot,
    },
    Assigned {
        word_offset: u32,
        slot: Slot,
        value: Value,
    },
    AwaitingReturn {
        word_offset: u32,
        slot: Slot,
    },
    Label {
        word_offset: u32,
        name: String,
    },
    Call {
        word_offset: u32,
        destination: u32,
        depth: usize,
    },
    Jump {
        word_offset: u32,
        destination: u32,
    },
    Returned {
        word_offset: u32,
        destination: Option<u32>,
        value: Option<Value>,
    },
    HostCall {
        word_offset: u32,
        function: String,
        arguments: Vec<Value>,
        destination: Option<Slot>,
    },
    Halted,
}

pub struct Vm {
    script: Arc<Script>,
    pub pc: u32,
    memory: BTreeMap<Slot, Value>,
    target: Option<Slot>,
    awaiting_return: bool,
    pending_call: Option<(Event, u32)>,
    frames: Vec<Frame>,
    default_banks: bool,
}

struct Frame {
    script: Arc<Script>,
    return_pc: u32,
    memory: BTreeMap<Slot, Value>,
    return_target: Option<Slot>,
}

/// Serializable execution state. Script content is resolved from game resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmCheckpoint {
    version: u32,
    script: String,
    script_fingerprint: String,
    pc: u32,
    memory: Vec<(Slot, Value)>,
    target: Option<Slot>,
    awaiting_return: bool,
    pending_call: Option<(Event, u32)>,
    frames: Vec<FrameCheckpoint>,
    default_banks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionOption {
    pub label: String,
    pub branch_pc: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameCheckpoint {
    script: String,
    script_fingerprint: String,
    return_pc: u32,
    memory: Vec<(Slot, Value)>,
    return_target: Option<Slot>,
}

/// Stable FNV-1a content identifier used to reject saves for changed scripts.
pub fn script_fingerprint(script: &Script) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    feed(b"yuka-script-v1");
    feed(&[match script.encoding {
        crate::script::TextEncoding::ShiftJis => 0,
        crate::script::TextEncoding::Gbk => 1,
        crate::script::TextEncoding::Utf8 => 2,
    }]);
    feed(&script.header.max_locals.to_le_bytes());
    feed(&(script.code.len() as u64).to_le_bytes());
    for word in &script.code {
        feed(&word.to_le_bytes());
    }
    feed(&(script.entries.len() as u64).to_le_bytes());
    for entry in &script.entries {
        feed(&entry.kind.to_le_bytes());
        for word in entry.fields {
            feed(&word.to_le_bytes());
        }
    }
    feed(&(script.data.len() as u64).to_le_bytes());
    feed(&script.data);
    format!("{hash:016x}")
}

fn restore_memory(
    entries: Vec<(Slot, Value)>,
    script: &Script,
    locals_only: bool,
) -> Result<BTreeMap<Slot, Value>> {
    let mut memory = BTreeMap::new();
    for (slot, value) in entries {
        validate_saved_slot(&slot, script)?;
        Vm::validate_assignment(&slot, &value)?;
        if locals_only && !frame_local(&slot) {
            return Err("saved local frame contains shared variables".into());
        }
        if memory.insert(slot, value).is_some() {
            return Err("saved memory contains duplicate slots".into());
        }
    }
    Ok(memory)
}

fn validate_saved_slot(slot: &Slot, script: &Script) -> Result<()> {
    match slot {
        Slot::Local { id } if *id >= script.header.max_locals => {
            Err("saved local ID exceeds MaxLocals".into())
        }
        Slot::DataWord { offset }
            if (*offset as usize)
                .checked_add(4)
                .is_none_or(|end| end > script.data.len()) =>
        {
            Err("saved data word is outside the script".into())
        }
        Slot::IntegerVariable { id, .. } | Slot::StringVariable { id, .. } if *id < 0 => {
            Err("saved variable ID is negative".into())
        }
        _ => Ok(()),
    }
}

fn load_saved_script(
    name: &str,
    expected: &str,
    load: &mut impl FnMut(&str) -> Result<Arc<Script>>,
) -> Result<Arc<Script>> {
    let script = load(name)?;
    if script_fingerprint(&script) != expected {
        return Err(format!("saved script content differs from current resource: {name}").into());
    }
    script.disassemble()?;
    Ok(script)
}

fn frame_local(slot: &Slot) -> bool {
    matches!(slot, Slot::Local { .. } | Slot::DataWord { .. })
}

impl Vm {
    pub fn new(script: &Script) -> Self {
        Self::new_owned(Arc::new(script.clone()))
    }

    pub fn new_owned(script: Arc<Script>) -> Self {
        Self {
            script,
            pc: 0,
            memory: BTreeMap::new(),
            target: None,
            awaiting_return: false,
            pending_call: None,
            frames: Vec::new(),
            default_banks: false,
        }
    }

    pub fn memory(&self) -> &BTreeMap<Slot, Value> {
        &self.memory
    }

    pub fn current_script(&self) -> &Arc<Script> {
        &self.script
    }

    pub fn pending_host_call(&self) -> Option<&Event> {
        self.pending_call.as_ref().map(|(event, _)| event)
    }

    /// Resolve each script to a game-resource name, without embedding game data.
    pub fn checkpoint(
        &self,
        resolve: impl Fn(&Arc<Script>) -> Result<String>,
    ) -> Result<VmCheckpoint> {
        let frames = self
            .frames
            .iter()
            .map(|frame| {
                Ok(FrameCheckpoint {
                    script: resolve(&frame.script)?,
                    script_fingerprint: script_fingerprint(&frame.script),
                    return_pc: frame.return_pc,
                    memory: frame
                        .memory
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    return_target: frame.return_target.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(VmCheckpoint {
            version: 1,
            script: resolve(&self.script)?,
            script_fingerprint: script_fingerprint(&self.script),
            pc: self.pc,
            memory: self
                .memory
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            target: self.target.clone(),
            awaiting_return: self.awaiting_return,
            pending_call: self.pending_call.clone(),
            frames,
            default_banks: self.default_banks,
        })
    }

    /// Build and validate a new VM. A failed restore leaves the caller's VM intact.
    pub fn restore(
        checkpoint: VmCheckpoint,
        mut load: impl FnMut(&str) -> Result<Arc<Script>>,
    ) -> Result<Self> {
        if checkpoint.version != 1 {
            return Err("unsupported VM checkpoint version".into());
        }
        let script = load_saved_script(
            &checkpoint.script,
            &checkpoint.script_fingerprint,
            &mut load,
        )?;
        let memory = restore_memory(checkpoint.memory, &script, false)?;
        if let Some(slot) = &checkpoint.target {
            validate_saved_slot(slot, &script)?;
        }
        if checkpoint.awaiting_return && checkpoint.target.is_none() {
            return Err("saved return assignment has no target".into());
        }
        let mut frames = Vec::new();
        for saved in checkpoint.frames {
            let script = load_saved_script(&saved.script, &saved.script_fingerprint, &mut load)?;
            let memory = restore_memory(saved.memory, &script, true)?;
            if let Some(slot) = &saved.return_target {
                validate_saved_slot(slot, &script)?;
            }
            let caller = Self::new_owned(Arc::clone(&script));
            caller.validate_boundary(saved.return_pc)?;
            let mut word = 0;
            let mut valid_return = false;
            while word < saved.return_pc {
                let (instruction, next) = script.instruction_at(word)?;
                valid_return = next == saved.return_pc
                    && matches!(&script.entry(instruction.entry_index)?.element,
                        Element::Function { name } if name.text == "ScriptCall")
                    && matches!(instruction.kind, InstructionKind::Call { arguments }
                        if arguments.len() == 1 && matches!(script.entry(arguments[0])?.element, Element::Control { .. }));
                word = next;
            }
            if !valid_return {
                return Err("saved frame return PC does not follow a local ScriptCall".into());
            }
            frames.push(Frame {
                script,
                return_pc: saved.return_pc,
                memory,
                return_target: saved.return_target,
            });
        }
        let mut vm = Self {
            script,
            pc: checkpoint.pc,
            memory,
            target: checkpoint.target,
            awaiting_return: checkpoint.awaiting_return,
            pending_call: None,
            frames,
            default_banks: checkpoint.default_banks,
        };
        vm.validate_boundary(vm.pc)?;
        if vm.pc as usize == vm.script.code.len() && vm.target.is_some() {
            return Err("saved script ended with an incomplete assignment".into());
        }
        if let Some((event, next)) = checkpoint.pending_call {
            if !matches!(event, Event::HostCall { .. }) {
                return Err("saved pending event is not a host call".into());
            }
            let actual = vm.step()?;
            if actual != event || vm.pending_call.as_ref().map(|(_, word)| *word) != Some(next) {
                return Err("saved pending host call differs from script state".into());
            }
        }
        Ok(vm)
    }

    /// Opt into the zero/empty initial state of the four standard engine banks.
    /// Inspection tools keep the stricter unknown-value behavior by default.
    pub fn initialize_default_banks(&mut self) {
        self.default_banks = true;
    }

    pub fn shared_memory(&self) -> BTreeMap<Slot, Value> {
        self.memory
            .iter()
            .filter(|(slot, _)| !frame_local(slot))
            .map(|(slot, value)| (slot.clone(), value.clone()))
            .collect()
    }

    pub fn replace_shared_memory(&mut self, memory: BTreeMap<Slot, Value>) -> Result<()> {
        for (slot, value) in &memory {
            if frame_local(slot) {
                return Err("shared memory contains a frame-local slot".into());
            }
            Self::validate_assignment(slot, value)?;
        }
        self.memory.retain(|slot, _| frame_local(slot));
        self.memory.extend(memory);
        Ok(())
    }

    pub fn set_variable(&mut self, slot: Slot, value: Value) -> Result<()> {
        Self::validate_assignment(&slot, &value)?;
        self.memory.insert(slot, value);
        Ok(())
    }

    fn default_value(&self, slot: &Slot) -> Option<Value> {
        if !self.default_banks {
            return None;
        }
        match slot {
            Slot::IntegerVariable { bank, id }
                if matches!(bank.as_str(), "GlobalFlag" | "Flag") && (0..10000).contains(id) =>
            {
                Some(Value::Integer(0))
            }
            Slot::StringVariable { bank, id }
                if matches!(bank.as_str(), "GlobalString" | "String")
                    && (0..65536).contains(id) =>
            {
                Some(Value::String(Text {
                    text: String::new(),
                    bytes: Vec::new(),
                    had_errors: false,
                }))
            }
            _ => None,
        }
    }

    fn integer_at(&self, offset: u32) -> Result<i32> {
        let slot = Slot::DataWord { offset };
        if let Some(Value::Integer(value)) = self.memory.get(&slot) {
            return Ok(*value);
        }
        let start = offset as usize;
        let end = start.checked_add(4).ok_or("data offset overflow")?;
        let bytes = self
            .script
            .data
            .get(start..end)
            .ok_or("data word outside script")?;
        Ok(i32::from_le_bytes(bytes.try_into()?))
    }

    fn slot(&self, index: u32) -> Result<Slot> {
        let entry = self.script.entry(index)?;
        Ok(match &entry.element {
            Element::IntegerVariable { bank, .. } => Slot::IntegerVariable {
                bank: bank.text.clone(),
                id: self.integer_at(entry.fields[2])?,
            },
            Element::StringVariable { bank, .. } => Slot::StringVariable {
                bank: bank.text.clone(),
                id: self.integer_at(entry.fields[2])?,
            },
            Element::Local { id } => {
                if *id >= self.script.header.max_locals {
                    return Err("local ID exceeds MaxLocals".into());
                }
                Slot::Local { id: *id }
            }
            Element::Integer { .. } => Slot::DataWord {
                offset: entry.fields[1],
            },
            Element::SpecialString { name } => Slot::SpecialString {
                name: name.text.clone(),
            },
            _ => return Err("unsupported assignment target".into()),
        })
    }

    fn value(&self, index: u32) -> Result<Value> {
        let entry = self.script.entry(index)?;
        match &entry.element {
            Element::Integer { .. } => Ok(Value::Integer(self.integer_at(entry.fields[1])?)),
            Element::String { value } => Ok(Value::String(value.clone())),
            Element::Control { name, .. } => Ok(Value::Label {
                name: name.text.clone(),
                link: self.integer_at(entry.fields[1])?,
            }),
            Element::IntegerVariable { .. }
            | Element::StringVariable { .. }
            | Element::Local { .. }
            | Element::SpecialString { .. } => {
                let slot = self.slot(index)?;
                self.memory
                    .get(&slot)
                    .cloned()
                    .or_else(|| self.default_value(&slot))
                    .ok_or_else(|| {
                        format!("variable has no verified initial value: {slot:?}").into()
                    })
            }
            _ => Err("unsupported function argument kind".into()),
        }
    }

    fn validate_assignment(slot: &Slot, value: &Value) -> Result<()> {
        let compatible = match slot {
            Slot::IntegerVariable { id, .. } => *id >= 0 && matches!(value, Value::Integer(_)),
            Slot::DataWord { .. } => matches!(value, Value::Integer(_)),
            Slot::StringVariable { id, .. } => *id >= 0 && matches!(value, Value::String(_)),
            Slot::SpecialString { .. } => matches!(value, Value::String(_)),
            Slot::Local { .. } => matches!(value, Value::Integer(_) | Value::String(_)),
        };
        if !compatible {
            return Err(format!("unsupported assignment value {value:?} for {slot:?}").into());
        }
        Ok(())
    }

    fn operator(&self, index: u32) -> Result<&str> {
        let Element::Control { name, link } = &self.script.entry(index)?.element else {
            return Err("expression operator is not a control symbol".into());
        };
        if *link != 65535 {
            return Err("expression control is not marked as an operator".into());
        }
        Ok(&name.text)
    }

    fn precedence(operator: &str) -> Result<u8> {
        match operator {
            "=" | "<" | ">" => Ok(7),
            "+" | "-" => Ok(8),
            "*" | "/" | "%" => Ok(9),
            _ => Err(format!("unimplemented expression operator {operator:?}").into()),
        }
    }

    fn calculate(operator: &str, left: Value, right: Value) -> Result<Value> {
        match (left, right) {
            (Value::Integer(left), Value::Integer(right)) => {
                let value = match operator {
                    "+" => left.wrapping_add(right),
                    "-" => left.wrapping_sub(right),
                    "*" => left.wrapping_mul(right),
                    "/" if left == 0 || right == 0 => 0,
                    "/" => left.checked_div(right).ok_or("integer division overflow")?,
                    "%" => left
                        .checked_rem(right)
                        .ok_or("integer remainder by zero or overflow")?,
                    "=" => i32::from(left == right),
                    "<" => i32::from(left < right),
                    ">" => i32::from(left > right),
                    _ => return Err(format!("unsupported integer operator {operator}").into()),
                };
                Ok(Value::Integer(value))
            }
            (Value::String(mut left), Value::String(right)) => match operator {
                "+" => {
                    left.text.push_str(&right.text);
                    left.bytes.extend(right.bytes);
                    left.had_errors |= right.had_errors;
                    Ok(Value::String(left))
                }
                "=" => Ok(Value::Integer(i32::from(left.bytes == right.bytes))),
                _ => Err(format!("unimplemented string operator {operator}").into()),
            },
            _ => Err("expression operands have incompatible types".into()),
        }
    }

    fn operand(&self, arguments: &[u32], cursor: &mut usize) -> Result<Value> {
        let index = *arguments
            .get(*cursor)
            .ok_or("expression ends before operand")?;
        *cursor += 1;
        if matches!(self.script.entry(index)?.element, Element::Control { .. }) {
            let operator = self.operator(index)?;
            if !matches!(operator, "+" | "-") {
                return Err("unsupported unary operator".into());
            }
            let value_index = *arguments
                .get(*cursor)
                .ok_or("unary operator has no operand")?;
            *cursor += 1;
            return Self::calculate(operator, Value::Integer(0), self.value(value_index)?);
        }
        self.value(index)
    }

    fn expression(&self, arguments: &[u32]) -> Result<Value> {
        let mut cursor = 0;
        let mut values = vec![self.operand(arguments, &mut cursor)?];
        let mut operators: Vec<&str> = Vec::new();
        while cursor < arguments.len() {
            let operator = self.operator(arguments[cursor])?;
            let precedence = Self::precedence(operator)?;
            cursor += 1;
            while let Some(previous) = operators.last() {
                if Self::precedence(previous)? < precedence {
                    break;
                }
                let previous = operators.pop().expect("operator exists");
                let right = values.pop().ok_or("missing right operand")?;
                let left = values.pop().ok_or("missing left operand")?;
                values.push(Self::calculate(previous, left, right)?);
            }
            operators.push(operator);
            values.push(self.operand(arguments, &mut cursor)?);
        }
        while let Some(operator) = operators.pop() {
            let right = values.pop().ok_or("missing right operand")?;
            let left = values.pop().ok_or("missing left operand")?;
            values.push(Self::calculate(operator, left, right)?);
        }
        values.pop().ok_or_else(|| "empty expression".into())
    }

    fn validate_boundary(&self, destination: u32) -> Result<()> {
        let mut boundary = 0;
        while boundary < destination {
            boundary = self.script.instruction_at(boundary)?.1;
        }
        if boundary != destination {
            return Err("control link points inside instruction operands".into());
        }
        Ok(())
    }

    fn block_link(&self, open_word: u32) -> Result<u32> {
        let (open, _) = self.script.instruction_at(open_word)?;
        let entry = self.script.entry(open.entry_index)?;
        let Element::Control { name, .. } = &entry.element else {
            return Err("condition is not followed by a block".into());
        };
        if name.text != "{" {
            return Err("condition is not followed by an opening brace".into());
        }
        let destination =
            u32::try_from(self.integer_at(entry.fields[1])?).map_err(|_| "negative block link")?;
        self.validate_boundary(destination)?;
        let (target, _) = self.script.instruction_at(destination)?;
        if !matches!(&self.script.entry(target.entry_index)?.element, Element::Control { name, .. } if matches!(name.text.as_str(), "{" | "}"))
        {
            return Err("block link does not point to a block boundary".into());
        }
        Ok(destination)
    }

    fn label_destination(&self, index: u32, fresh_frame: bool) -> Result<u32> {
        let entry = self.script.entry(index)?;
        let Element::Control { name, link } = &entry.element else {
            return Err("local control transfer requires a label argument".into());
        };
        let link = if fresh_frame {
            *link
        } else {
            self.integer_at(entry.fields[1])?
        };
        let destination = u32::try_from(link).map_err(|_| "unresolved negative label link")?;
        self.validate_boundary(destination)?;
        let (target, _) = self.script.instruction_at(destination)?;
        let Element::Control {
            name: target_name, ..
        } = &self.script.entry(target.entry_index)?.element
        else {
            return Err("label link does not point to a control instruction".into());
        };
        if name.bytes != target_name.bytes {
            return Err("label link points to a different label".into());
        }
        Ok(destination)
    }

    fn return_from_frame(&mut self, value: Option<Value>) -> Result<Event> {
        let word_offset = self.pc;
        if self.target.is_some() {
            return Err("return encountered while an assignment is incomplete".into());
        }
        if let Some(frame) = self.frames.last()
            && let Some(slot) = &frame.return_target
        {
            Self::validate_assignment(
                slot,
                value
                    .as_ref()
                    .ok_or("called script returned no value for pending assignment")?,
            )?;
        }
        let destination = match self.frames.pop() {
            Some(frame) => {
                self.script = frame.script;
                self.memory.retain(|slot, _| !frame_local(slot));
                self.memory.extend(frame.memory);
                if let Some(slot) = frame.return_target {
                    self.memory
                        .insert(slot, value.clone().expect("validated return value"));
                }
                self.pc = frame.return_pc;
                Some(self.pc)
            }
            None => {
                self.pc = self.script.code.len() as u32;
                None
            }
        };
        Ok(Event::Returned {
            word_offset,
            destination,
            value,
        })
    }

    /// A pending host call is returned again until explicitly completed by the host.
    pub fn step(&mut self) -> Result<Event> {
        if let Some((event, _)) = &self.pending_call {
            return Ok(event.clone());
        }
        if self.pc as usize == self.script.code.len() {
            if self.target.is_some() {
                return Err("script ended with an unassigned target".into());
            }
            if !self.frames.is_empty() {
                return self.return_from_frame(None);
            }
            return Ok(Event::Halted);
        }
        let (instruction, next) = self.script.instruction_at(self.pc)?;
        let entry = self.script.entry(instruction.entry_index)?;
        let event = match instruction.kind {
            InstructionKind::Target => {
                if self.target.is_some() {
                    return Err("assignment target already pending".into());
                }
                let slot = self.slot(instruction.entry_index)?;
                self.target = Some(slot.clone());
                Event::Target {
                    word_offset: self.pc,
                    slot,
                }
            }
            InstructionKind::Label => {
                if self.target.is_some() {
                    return Err("label encountered while assignment target is pending".into());
                }
                let Element::Control { name, .. } = &entry.element else {
                    unreachable!()
                };
                if name.text == "else" {
                    let end = self.block_link(next)?;
                    let (closing, destination) = self.script.instruction_at(end)?;
                    if !matches!(&self.script.entry(closing.entry_index)?.element, Element::Control { name, .. } if name.text == "}")
                    {
                        return Err("else block does not link to its closing brace".into());
                    }
                    let word_offset = self.pc;
                    self.pc = destination;
                    return Ok(Event::Jump {
                        word_offset,
                        destination,
                    });
                }
                Event::Label {
                    word_offset: self.pc,
                    name: name.text.clone(),
                }
            }
            InstructionKind::Call { arguments } => {
                let Element::Function { name } = &entry.element else {
                    unreachable!()
                };
                if name.text == "=" {
                    if self.awaiting_return {
                        return Err("expected host function after parameterless assignment".into());
                    }
                    let slot = self.target.clone().ok_or("assignment has no target")?;
                    match arguments.as_slice() {
                        [] => {
                            self.awaiting_return = true;
                            Event::AwaitingReturn {
                                word_offset: self.pc,
                                slot,
                            }
                        }
                        _ => {
                            let value = self.expression(&arguments)?;
                            Self::validate_assignment(&slot, &value)?;
                            self.memory.insert(slot.clone(), value.clone());
                            self.target = None;
                            Event::Assigned {
                                word_offset: self.pc,
                                slot,
                                value,
                            }
                        }
                    }
                } else {
                    if self.target.is_some() && !self.awaiting_return {
                        return Err("host call encountered before assignment operator".into());
                    }
                    if matches!(name.text.as_str(), "If" | "if") {
                        if self.target.is_some() {
                            return Err("If cannot return an assignment value".into());
                        }
                        let [argument] = arguments.as_slice() else {
                            return Err("If requires one condition".into());
                        };
                        let Value::Integer(condition) = self.value(*argument)? else {
                            return Err("If condition must be an integer".into());
                        };
                        let end = self.block_link(next)?;
                        let destination = if condition == 0 { end } else { next };
                        let word_offset = self.pc;
                        self.pc = destination;
                        return Ok(Event::Jump {
                            word_offset,
                            destination,
                        });
                    }
                    if name.text == "Return" {
                        let value = match arguments.as_slice() {
                            [] => None,
                            [argument] => {
                                let value = self.value(*argument)?;
                                if !matches!(value, Value::Integer(_)) {
                                    return Err("only integer Return arguments are verified".into());
                                }
                                Some(value)
                            }
                            _ => return Err("Return accepts zero or one verified argument".into()),
                        };
                        return self.return_from_frame(value);
                    }
                    if name.text == "ScriptJump" && self.target.is_some() {
                        return Err("ScriptJump cannot complete pending assignment".into());
                    }
                    if matches!(name.text.as_str(), "ScriptCall" | "ScriptJump")
                        && arguments.len() == 1
                        && matches!(
                            self.script.entry(arguments[0])?.element,
                            Element::Control { .. }
                        )
                    {
                        let destination =
                            self.label_destination(arguments[0], name.text == "ScriptCall")?;
                        let word_offset = self.pc;
                        if name.text == "ScriptCall" {
                            // The native loader creates fresh code/index/data and local
                            // registers for each call. Flag/String banks remain shared.
                            let (local, shared) = std::mem::take(&mut self.memory)
                                .into_iter()
                                .partition(|(slot, _)| frame_local(slot));
                            self.memory = shared;
                            self.frames.push(Frame {
                                script: Arc::clone(&self.script),
                                return_pc: next,
                                memory: local,
                                return_target: self.target.take(),
                            });
                            self.awaiting_return = false;
                            self.pc = destination;
                            return Ok(Event::Call {
                                word_offset,
                                destination,
                                depth: self.frames.len(),
                            });
                        }
                        self.pc = destination;
                        return Ok(Event::Jump {
                            word_offset,
                            destination,
                        });
                    }
                    let values = arguments
                        .iter()
                        .map(|&index| self.value(index))
                        .collect::<Result<Vec<_>>>()?;
                    let event = Event::HostCall {
                        word_offset: self.pc,
                        function: name.text.clone(),
                        arguments: values,
                        destination: self.target.clone(),
                    };
                    self.pending_call = Some((event.clone(), next));
                    return Ok(event);
                }
            }
        };
        self.pc = next;
        Ok(event)
    }

    /// Completes an ordinary returning host call. ScriptCall/ScriptJump and other
    /// control transfers require an engine-level frame scheduler before resuming.
    pub fn resume_host(&mut self, value: Option<Value>) -> Result<()> {
        let (event, _) = self
            .pending_call
            .as_ref()
            .ok_or("no host call is pending")?;
        let Event::HostCall {
            function,
            arguments,
            ..
        } = event
        else {
            unreachable!()
        };
        if matches!(
            function.as_str(),
            "ScriptCall"
                | "ScriptJump"
                | "Return"
                | "If"
                | "if"
                | "While"
                | "while"
                | "Loop"
                | "End"
                | "Exit"
                | "ScriptExit"
                | "Restart"
                | "Select"
                | "Case"
                | "Case2"
                | "MouseCase"
                | "MouseCase2"
                | "MouseCase2Already"
                | "MouseKeyCase"
                | "KeyCase"
                | "KeyTableCase"
                | "IndicateUsualCase"
                | "IndicateCase"
                | "IndicateUsualTimeCase"
                | "TogetherIndicateCase"
                | "WindowMessageCase"
        ) || arguments
            .iter()
            .any(|value| matches!(value, Value::Label { .. }))
        {
            return Err(
                format!("{function} requires unimplemented engine control-flow semantics").into(),
            );
        }
        self.complete_pending(value)
    }

    /// Read the direct Case children of a suspended Select's following block.
    /// Case geometry remains a host concern. The VM preserves each branch PC.
    pub fn selection_options(&self) -> Result<Vec<SelectionOption>> {
        let (Event::HostCall { function, .. }, next) = self
            .pending_call
            .as_ref()
            .ok_or("no selection is pending")?
        else {
            return Err("pending event is not a host call".into());
        };
        if function != "Select" || self.target.is_some() {
            return Err("pending call is not a standalone Select".into());
        }
        let end = self.block_link(*next)?;
        let (closing, _) = self.script.instruction_at(end)?;
        if !matches!(&self.script.entry(closing.entry_index)?.element, Element::Control { name, .. } if name.text == "}")
        {
            return Err("Select block has no matching closing brace".into());
        }
        let mut options = Vec::new();
        let mut depth = 0usize;
        let mut word = *next;
        while word < end {
            let (instruction, next_word) = self.script.instruction_at(word)?;
            let entry = self.script.entry(instruction.entry_index)?;
            match (&entry.element, instruction.kind) {
                (Element::Control { name, .. }, _) if name.text == "{" => depth += 1,
                (Element::Control { name, .. }, _) if name.text == "}" => {
                    depth = depth.checked_sub(1).ok_or("unbalanced Select block")?;
                    if depth == 0 {
                        return Err("Select block closes before its linked end".into());
                    }
                }
                (Element::Function { name }, InstructionKind::Call { arguments })
                    if name.text == "Case" && depth == 1 =>
                {
                    let first = *arguments.first().ok_or("Case has no option text")?;
                    let Value::String(label) = self.value(first)? else {
                        return Err("Case option text is not a string".into());
                    };
                    let branch_end = self.block_link(next_word)?;
                    let (branch_close, _) = self.script.instruction_at(branch_end)?;
                    if branch_end <= next_word
                        || branch_end >= end
                        || !matches!(
                            &self.script.entry(branch_close.entry_index)?.element,
                            Element::Control { name, .. } if name.text == "}"
                        )
                    {
                        return Err("Case body extends outside Select block".into());
                    }
                    options.push(SelectionOption {
                        label: label.text,
                        branch_pc: next_word,
                    });
                }
                _ => {}
            }
            word = next_word;
        }
        if depth != 1 || options.is_empty() {
            return Err("Select has no valid Case options or unbalanced braces".into());
        }
        Ok(options)
    }

    /// Continue at the selected Case's opening brace, matching the native Select handler.
    pub fn resume_selection(&mut self, index: usize) -> Result<()> {
        let options = self.selection_options()?;
        let selected = options
            .get(index)
            .ok_or("selection index is out of range")?;
        self.pc = selected.branch_pc;
        self.pending_call = None;
        self.awaiting_return = false;
        Ok(())
    }

    /// The scheduler calls this only after a string-named ScriptCall child has
    /// returned and its shared banks have been copied back into this VM.
    pub fn resume_script_call(&mut self, value: Option<Value>) -> Result<()> {
        let (
            Event::HostCall {
                function,
                arguments,
                ..
            },
            _,
        ) = self
            .pending_call
            .as_ref()
            .ok_or("no script call is pending")?
        else {
            return Err("pending event is not a host call".into());
        };
        if function != "ScriptCall" || !matches!(arguments.as_slice(), [Value::String(_)]) {
            return Err("pending call is not a single-path external ScriptCall".into());
        }
        self.complete_pending(value)
    }

    /// Replaces the active frame after the scheduler loads an external ScriptJump.
    /// Earlier same-file call frames retain their own script and return address.
    pub fn resume_script_jump(&mut self, script: Arc<Script>) -> Result<()> {
        let (
            Event::HostCall {
                function,
                arguments,
                ..
            },
            _,
        ) = self
            .pending_call
            .as_ref()
            .ok_or("no script jump is pending")?
        else {
            return Err("pending event is not a host call".into());
        };
        if function != "ScriptJump" || !matches!(arguments.as_slice(), [Value::String(_)]) {
            return Err("pending call is not a single-path external ScriptJump".into());
        }
        if self.target.is_some() {
            return Err("ScriptJump cannot complete pending assignment".into());
        }
        self.script = script;
        self.pc = 0;
        self.memory.retain(|slot, _| !frame_local(slot));
        self.pending_call = None;
        self.awaiting_return = false;
        Ok(())
    }

    fn complete_pending(&mut self, value: Option<Value>) -> Result<()> {
        let (_, next) = self
            .pending_call
            .as_ref()
            .ok_or("no host call is pending")?;
        if let Some(slot) = &self.target {
            let value =
                value.ok_or("host call must provide a return value for pending assignment")?;
            Self::validate_assignment(slot, &value)?;
            self.memory.insert(slot.clone(), value);
        }
        self.pc = *next;
        self.pending_call = None;
        self.target = None;
        self.awaiting_return = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{Header, IndexEntry, TextEncoding};

    fn text(value: &str) -> Text {
        Text {
            text: value.into(),
            bytes: value.as_bytes().to_vec(),
            had_errors: false,
        }
    }

    fn script() -> Script {
        Script {
            header: Header {
                encryption: 0,
                header_size: 48,
                unknown1: 0,
                code_offset: 48,
                code_count: 7,
                index_offset: 76,
                index_count: 4,
                data_offset: 140,
                data_length: 8,
                max_locals: 0,
                unknown2: 0,
            },
            encoding: TextEncoding::ShiftJis,
            code: vec![0, 1, 1, 2, 3, 1, 0],
            entries: vec![
                IndexEntry {
                    kind: 8,
                    fields: [0, 0, 0],
                    element: Element::IntegerVariable {
                        bank: text("GlobalFlag"),
                        id: 7,
                    },
                },
                IndexEntry {
                    kind: 0,
                    fields: [0, 0, 0],
                    element: Element::Function { name: text("=") },
                },
                IndexEntry {
                    kind: 4,
                    fields: [0, 4, 0],
                    element: Element::Integer { value: 42 },
                },
                IndexEntry {
                    kind: 0,
                    fields: [0, 0, 0],
                    element: Element::Function {
                        name: text("HostFunction"),
                    },
                },
            ],
            data: [7i32.to_le_bytes(), 42i32.to_le_bytes()].concat(),
        }
    }

    #[test]
    fn assignment_reaches_host_boundary_and_waits_for_resume() {
        let script = script();
        let mut vm = Vm::new(&script);
        assert!(matches!(vm.step().unwrap(), Event::Target { .. }));
        assert!(matches!(
            vm.step().unwrap(),
            Event::Assigned {
                value: Value::Integer(42),
                ..
            }
        ));
        assert!(
            matches!(vm.step().unwrap(), Event::HostCall { arguments, .. } if matches!(arguments[..], [Value::Integer(42)]))
        );
        assert_eq!(vm.pc, 4);
        assert!(matches!(vm.step().unwrap(), Event::HostCall { .. }));
        vm.resume_host(None).unwrap();
        assert!(matches!(vm.step().unwrap(), Event::Halted));
    }

    #[test]
    fn unresolved_value_stops_without_advancing() {
        let mut script = script();
        script.code = vec![3, 1, 0];
        let mut vm = Vm::new(&script);
        assert!(vm.step().is_err());
        assert_eq!(vm.pc, 0);
    }

    #[test]
    fn refuses_to_resume_control_transfer_as_ordinary_host_call() {
        let mut script = script();
        script.code = vec![3, 0];
        script.entries[3].element = Element::Function {
            name: text("ScriptJump"),
        };
        let mut vm = Vm::new(&script);
        assert!(matches!(vm.step().unwrap(), Event::HostCall { .. }));
        assert!(vm.resume_host(None).is_err());
        assert_eq!(vm.pc, 0);
    }

    #[test]
    fn local_call_has_fresh_data_restores_caller_and_shares_flag_bank() {
        let mut script = script();
        // Mutate caller constant 42 -> 99, call SUB, then pass restored constant
        // 99 and shared flag 42 to the host. SUB reads fresh constant 42.
        script.code = vec![2, 1, 1, 7, 4, 1, 5, 3, 2, 2, 0, 6, 0, 5, 0, 1, 1, 2, 6, 0];
        script.data.extend(13i32.to_le_bytes());
        script.data.extend(99i32.to_le_bytes());
        script.entries.extend([
            IndexEntry {
                kind: 0,
                fields: [0, 0, 0],
                element: Element::Function {
                    name: text("ScriptCall"),
                },
            },
            IndexEntry {
                kind: 1,
                fields: [0, 8, 0],
                element: Element::Control {
                    name: text("SUB"),
                    link: 13,
                },
            },
            IndexEntry {
                kind: 0,
                fields: [0, 0, 0],
                element: Element::Function {
                    name: text("Return"),
                },
            },
            IndexEntry {
                kind: 4,
                fields: [0, 12, 0],
                element: Element::Integer { value: 99 },
            },
        ]);
        let mut vm = Vm::new(&script);
        vm.step().unwrap();
        vm.step().unwrap();
        assert!(matches!(
            vm.step().unwrap(),
            Event::Call {
                destination: 13,
                depth: 1,
                ..
            }
        ));
        vm.step().unwrap(); // SUB label
        vm.step().unwrap(); // GlobalFlag target
        assert!(matches!(
            vm.step().unwrap(),
            Event::Assigned {
                value: Value::Integer(42),
                ..
            }
        ));
        assert!(matches!(
            vm.step().unwrap(),
            Event::Returned {
                destination: Some(7),
                ..
            }
        ));
        assert!(
            matches!(vm.step().unwrap(), Event::HostCall { arguments, .. } if matches!(arguments[..], [Value::Integer(99), Value::Integer(42)]))
        );
    }

    #[test]
    fn rejects_label_link_into_an_argument_even_when_it_decodes_as_label() {
        let mut script = script();
        script.code = vec![3, 1, 4];
        script.data.extend(2i32.to_le_bytes());
        script.entries[3].element = Element::Function {
            name: text("ScriptJump"),
        };
        script.entries.push(IndexEntry {
            kind: 1,
            fields: [0, 8, 0],
            element: Element::Control {
                name: text("BAD"),
                link: 2,
            },
        });
        let mut vm = Vm::new(&script);
        assert!(
            vm.step()
                .unwrap_err()
                .to_string()
                .contains("inside instruction operands")
        );
        assert_eq!(vm.pc, 0);
    }

    #[test]
    fn refuses_all_verified_control_hosts_and_label_arguments() {
        for function in [
            "ScriptExit",
            "Restart",
            "Case",
            "Case2",
            "MouseCase2Already",
            "WindowMessageCase",
        ] {
            let mut script = script();
            script.code = vec![3, 0];
            script.entries[3].element = Element::Function {
                name: text(function),
            };
            let mut vm = Vm::new(&script);
            vm.step().unwrap();
            assert!(vm.resume_host(None).is_err(), "{function}");
            assert_eq!(vm.pc, 0);
        }
        let mut script = script();
        script.code = vec![3, 1, 4];
        script.data.extend((-1i32).to_le_bytes());
        script.entries.push(IndexEntry {
            kind: 1,
            fields: [0, 8, 0],
            element: Element::Control {
                name: text("CALLBACK"),
                link: -1,
            },
        });
        let mut vm = Vm::new(&script);
        vm.step().unwrap();
        assert!(vm.resume_host(None).is_err());
        assert_eq!(vm.pc, 0);
    }

    fn add_integer(script: &mut Script, value: i32) -> u32 {
        let index = script.entries.len() as u32;
        let offset = script.data.len() as u32;
        script.data.extend(value.to_le_bytes());
        script.entries.push(IndexEntry {
            kind: 4,
            fields: [0, offset, 0],
            element: Element::Integer { value },
        });
        index
    }

    fn add_control(script: &mut Script, name: &str, link: i32) -> u32 {
        let index = script.entries.len() as u32;
        let offset = script.data.len() as u32;
        script.data.extend(link.to_le_bytes());
        script.entries.push(IndexEntry {
            kind: 1,
            fields: [0, offset, 0],
            element: Element::Control {
                name: text(name),
                link,
            },
        });
        index
    }

    #[test]
    fn expressions_follow_native_precedence_and_preserve_failure_position() {
        let mut script = script();
        let two = add_integer(&mut script, 2);
        let three = add_integer(&mut script, 3);
        let four = add_integer(&mut script, 4);
        let zero = add_integer(&mut script, 0);
        let plus = add_control(&mut script, "+", 65535);
        let minus = add_control(&mut script, "-", 65535);
        let multiply = add_control(&mut script, "*", 65535);
        let divide = add_control(&mut script, "/", 65535);
        let remainder = add_control(&mut script, "%", 65535);
        let equal = add_control(&mut script, "=", 65535);
        script.code = vec![0, 1, 5, two, plus, three, multiply, four];
        let mut vm = Vm::new(&script);
        vm.step().unwrap();
        assert!(matches!(
            vm.step().unwrap(),
            Event::Assigned {
                value: Value::Integer(14),
                ..
            }
        ));
        assert!(matches!(
            vm.expression(&[two, minus, three, minus, four]).unwrap(),
            Value::Integer(-5)
        ));
        assert!(matches!(
            vm.expression(&[minus, two, multiply, three]).unwrap(),
            Value::Integer(-6)
        ));
        assert!(matches!(
            vm.expression(&[two, divide, zero]).unwrap(),
            Value::Integer(0)
        ));
        assert!(matches!(
            vm.expression(&[two, equal, three, minus, four]).unwrap(),
            Value::Integer(0)
        ));
        assert!(vm.expression(&[two, plus]).is_err());
        script.code = vec![0, 1, 3, two, remainder, zero];
        let mut vm = Vm::new(&script);
        vm.step().unwrap();
        assert!(vm.step().is_err());
        assert_eq!(vm.pc, 1);
        assert!(vm.memory().is_empty());
    }

    #[test]
    fn string_expressions_concatenate_original_bytes_and_compare_them() {
        let left = text("abc");
        let mut right = text("def");
        right.had_errors = true;
        let joined = Vm::calculate("+", Value::String(left.clone()), Value::String(right)).unwrap();
        let Value::String(joined) = joined else {
            panic!("expected string")
        };
        assert_eq!(joined.text, "abcdef");
        assert_eq!(joined.bytes, b"abcdef");
        assert!(joined.had_errors);
        assert!(matches!(
            Vm::calculate("=", Value::String(left.clone()), Value::String(left)).unwrap(),
            Value::Integer(1)
        ));
        assert!(Vm::calculate("+", Value::String(text("abc")), Value::Integer(2)).is_err());
    }

    #[test]
    fn if_else_uses_compiled_brace_links_and_executes_only_one_branch() {
        for (condition, expected) in [(0, 22), (1, 11)] {
            let mut script = script();
            script.entries[3].element = Element::Function { name: text("if") };
            let condition = add_integer(&mut script, condition);
            let then_value = add_integer(&mut script, 11);
            let else_value = add_integer(&mut script, 22);
            let then_open = add_control(&mut script, "{", 10);
            let then_close = add_control(&mut script, "}", 3);
            let else_marker = add_control(&mut script, "else", -1);
            let else_open = add_control(&mut script, "{", 15);
            let else_close = add_control(&mut script, "}", 10);
            script.code = vec![
                3,
                1,
                condition,
                then_open,
                0,
                1,
                1,
                then_value,
                then_close,
                else_marker,
                else_open,
                0,
                1,
                1,
                else_value,
                else_close,
            ];
            let mut vm = Vm::new(&script);
            let mut assignments = 0;
            loop {
                match vm.step().unwrap() {
                    Event::Assigned {
                        value: Value::Integer(value),
                        ..
                    } => {
                        assignments += 1;
                        assert_eq!(value, expected);
                    }
                    Event::Halted => break,
                    Event::HostCall { .. } => panic!("conditional must execute in the VM"),
                    _ => {}
                }
            }
            assert_eq!(assignments, 1);
        }
    }

    #[test]
    fn external_call_resume_shares_banks_and_keeps_caller_data() {
        let mut script = script();
        script.entries[3].element = Element::Function {
            name: text("ScriptCall"),
        };
        let path = script.entries.len() as u32;
        script.entries.push(IndexEntry {
            kind: 5,
            fields: [0, 0, 0],
            element: Element::String {
                value: text("child.yks"),
            },
        });
        script.code = vec![0, 1, 0, 3, 1, path];
        let script = Arc::new(script);
        let mut vm = Vm::new_owned(Arc::clone(&script));
        vm.initialize_default_banks();
        assert!(matches!(vm.value(0).unwrap(), Value::Integer(0)));
        vm.set_variable(Slot::DataWord { offset: 4 }, Value::Integer(99))
            .unwrap();
        vm.step().unwrap();
        vm.step().unwrap();
        assert!(matches!(vm.step().unwrap(), Event::HostCall { .. }));
        vm = checkpoint_round_trip(&vm, &[("caller.yks", Arc::clone(&script))]);
        assert!(vm.resume_script_call(None).is_err());
        assert_eq!(vm.pc, 3);
        let mut child_memory = vm.shared_memory();
        assert!(child_memory.is_empty());
        let flag = Slot::IntegerVariable {
            bank: "Flag".into(),
            id: 210,
        };
        child_memory.insert(flag.clone(), Value::Integer(1));
        vm.replace_shared_memory(child_memory).unwrap();
        vm.resume_script_call(Some(Value::Integer(8))).unwrap();
        assert!(matches!(vm.value(0).unwrap(), Value::Integer(8)));
        assert!(matches!(vm.value(2).unwrap(), Value::Integer(99)));
        assert!(matches!(vm.memory().get(&flag), Some(Value::Integer(1))));
        assert!(matches!(vm.step().unwrap(), Event::Halted));
        let mut invalid = vm.shared_memory();
        invalid.insert(Slot::Local { id: 0 }, Value::Integer(1));
        assert!(vm.replace_shared_memory(invalid).is_err());
        assert!(matches!(vm.memory().get(&flag), Some(Value::Integer(1))));
    }

    #[test]
    fn external_jump_preserves_local_caller_script_and_shared_banks() {
        let mut caller = script();
        caller.entries[3].element = Element::Function {
            name: text("ScriptCall"),
        };
        let sub = add_control(&mut caller, "SUB", 5);
        let jump = caller.entries.len() as u32;
        caller.entries.push(IndexEntry {
            kind: 0,
            fields: [0, 0, 0],
            element: Element::Function {
                name: text("ScriptJump"),
            },
        });
        let path = caller.entries.len() as u32;
        caller.entries.push(IndexEntry {
            kind: 5,
            fields: [0, 0, 0],
            element: Element::String {
                value: text("jump.yks"),
            },
        });
        let return_function = caller.entries.len() as u32;
        caller.entries.push(IndexEntry {
            kind: 0,
            fields: [0, 0, 0],
            element: Element::Function {
                name: text("Return"),
            },
        });
        caller.code = vec![3, 1, sub, return_function, 0, sub, jump, 1, path];
        let mut replacement = script();
        replacement.code = vec![0, 1, 1, 2];
        let caller = Arc::new(caller);
        let replacement = Arc::new(replacement);
        let sources = [
            ("caller.yks", Arc::clone(&caller)),
            ("jump.yks", Arc::clone(&replacement)),
        ];
        let mut vm = Vm::new_owned(caller);
        vm.set_variable(Slot::DataWord { offset: 4 }, Value::Integer(99))
            .unwrap();
        assert!(matches!(vm.step().unwrap(), Event::Call { .. }));
        vm = checkpoint_round_trip(&vm, &sources);
        vm.step().unwrap();
        assert!(matches!(vm.step().unwrap(), Event::HostCall { .. }));
        vm = checkpoint_round_trip(&vm, &sources);
        assert!(vm.resume_script_call(None).is_err());
        vm.resume_script_jump(replacement).unwrap();
        vm = checkpoint_round_trip(&vm, &sources);
        vm.step().unwrap();
        assert!(matches!(
            vm.step().unwrap(),
            Event::Assigned {
                value: Value::Integer(42),
                ..
            }
        ));
        assert!(matches!(
            vm.step().unwrap(),
            Event::Returned {
                destination: Some(3),
                ..
            }
        ));
        vm = checkpoint_round_trip(&vm, &sources);
        assert!(matches!(vm.value(2).unwrap(), Value::Integer(99)));
        assert!(matches!(vm.value(0).unwrap(), Value::Integer(42)));
        assert!(matches!(
            vm.step().unwrap(),
            Event::Returned {
                destination: None,
                ..
            }
        ));
    }

    fn checkpoint_round_trip(vm: &Vm, sources: &[(&str, Arc<Script>)]) -> Vm {
        let snapshot = vm
            .checkpoint(|script| {
                sources
                    .iter()
                    .find(|(_, loaded)| Arc::ptr_eq(script, loaded))
                    .map(|(name, _)| name.to_string())
                    .ok_or_else(|| "unknown source".into())
            })
            .unwrap();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let decoded = serde_json::from_slice(&bytes).unwrap();
        Vm::restore(decoded, |name| {
            sources
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, script)| Arc::clone(script))
                .ok_or_else(|| "source missing".into())
        })
        .unwrap()
    }

    #[test]
    fn checkpoint_round_trip_preserves_each_execution_boundary() {
        let script = Arc::new(script());
        let mut original = Vm::new_owned(Arc::clone(&script));
        original.initialize_default_banks();
        original
            .set_variable(
                Slot::StringVariable {
                    bank: "String".into(),
                    id: 20,
                },
                Value::String(Text {
                    text: "爱泽".into(),
                    bytes: vec![0xb0, 0xae, 0xd4, 0xf3],
                    had_errors: false,
                }),
            )
            .unwrap();
        for _ in 0..4 {
            let mut restored =
                checkpoint_round_trip(&original, &[("main.yks", Arc::clone(&script))]);
            assert_eq!(original.memory(), restored.memory());
            let event = original.step().unwrap();
            assert_eq!(restored.step().unwrap(), event);
            if matches!(event, Event::HostCall { .. }) {
                restored = checkpoint_round_trip(&restored, &[("main.yks", Arc::clone(&script))]);
                original.resume_host(None).unwrap();
                restored.resume_host(None).unwrap();
                assert_eq!(restored.pc, original.pc);
            }
        }
    }

    #[test]
    fn checkpoint_rejects_corrupt_positions_memory_and_pending_calls() {
        let script = Arc::new(script());
        let mut vm = Vm::new_owned(Arc::clone(&script));
        vm.step().unwrap();
        vm.step().unwrap();
        vm.step().unwrap();
        let valid = vm.checkpoint(|_| Ok("main.yks".into())).unwrap();
        let restore = |checkpoint| Vm::restore(checkpoint, |_| Ok(Arc::clone(&script)));
        let mut bad = valid.clone();
        bad.pc = 2;
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.pc = u32::MAX;
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.pending_call = Some((Event::Halted, 7));
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.pending_call.as_mut().unwrap().1 = 4;
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        if let Event::HostCall { arguments, .. } = &mut bad.pending_call.as_mut().unwrap().0 {
            arguments[0] = Value::Integer(99);
        }
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.memory.push(bad.memory[0].clone());
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.memory[0].1 = Value::String(text("wrong type"));
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.memory
            .push((Slot::DataWord { offset: u32::MAX }, Value::Integer(0)));
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.memory.push((Slot::Local { id: 0 }, Value::Integer(0)));
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.awaiting_return = true;
        assert!(restore(bad).is_err());
        let mut changed = script.as_ref().clone();
        changed.data[0] ^= 1;
        assert!(Vm::restore(valid.clone(), |_| Ok(Arc::new(changed.clone()))).is_err());
        assert!(restore(valid).is_ok());
        assert_eq!(vm.pc, 4);
    }

    #[test]
    fn selection_resumes_exact_case_and_survives_checkpoint() {
        let mut script = script();
        script.entries[3].element = Element::Function {
            name: text("Select"),
        };
        let case = script.entries.len() as u32;
        script.entries.push(IndexEntry {
            kind: 0,
            fields: [0; 3],
            element: Element::Function { name: text("Case") },
        });
        let return_function = script.entries.len() as u32;
        script.entries.push(IndexEntry {
            kind: 0,
            fields: [0; 3],
            element: Element::Function {
                name: text("Return"),
            },
        });
        let first_label = script.entries.len() as u32;
        script.entries.push(IndexEntry {
            kind: 5,
            fields: [0; 3],
            element: Element::String {
                value: text("第一项"),
            },
        });
        let second_label = script.entries.len() as u32;
        script.entries.push(IndexEntry {
            kind: 5,
            fields: [0; 3],
            element: Element::String {
                value: text("第二项"),
            },
        });
        let second_value = add_integer(&mut script, 99);
        let outer = add_control(&mut script, "{", 25);
        let outer_close = add_control(&mut script, "}", 2);
        let first_body = add_control(&mut script, "{", 13);
        let first_close = add_control(&mut script, "}", 6);
        let second_body = add_control(&mut script, "{", 24);
        let second_close = add_control(&mut script, "}", 17);
        script.code = vec![
            3,
            0,
            outer,
            case,
            1,
            first_label,
            first_body,
            0,
            1,
            1,
            2,
            return_function,
            0,
            first_close,
            case,
            1,
            second_label,
            second_body,
            0,
            1,
            1,
            second_value,
            return_function,
            0,
            second_close,
            outer_close,
        ];
        let script = Arc::new(script);
        for (index, expected) in [(0, 42), (1, 99)] {
            let mut vm = Vm::new_owned(Arc::clone(&script));
            vm.step().unwrap();
            assert_eq!(
                vm.selection_options().unwrap(),
                vec![
                    SelectionOption {
                        label: "第一项".into(),
                        branch_pc: 6
                    },
                    SelectionOption {
                        label: "第二项".into(),
                        branch_pc: 17
                    },
                ]
            );
            assert!(vm.resume_host(None).is_err());
            assert!(vm.resume_selection(2).is_err());
            assert_eq!(vm.pc, 0);
            vm = checkpoint_round_trip(&vm, &[("select.yks", Arc::clone(&script))]);
            vm.resume_selection(index).unwrap();
            vm = checkpoint_round_trip(&vm, &[("select.yks", Arc::clone(&script))]);
            let mut assignments = 0;
            loop {
                match vm.step().unwrap() {
                    Event::Assigned { value, .. } => {
                        assignments += 1;
                        assert_eq!(value, Value::Integer(expected));
                    }
                    Event::Returned {
                        destination: None, ..
                    } => break,
                    Event::HostCall { .. } => panic!("selected branch must execute in VM"),
                    _ => {}
                }
            }
            assert_eq!(assignments, 1);
        }
    }

    #[test]
    fn checkpoint_rejects_corrupt_local_frames() {
        let mut script = script();
        script.header.max_locals = 1;
        script.entries[3].element = Element::Function {
            name: text("ScriptCall"),
        };
        let sub = add_control(&mut script, "SUB", 5);
        let return_function = script.entries.len() as u32;
        script.entries.push(IndexEntry {
            kind: 0,
            fields: [0; 3],
            element: Element::Function {
                name: text("Return"),
            },
        });
        script.code = vec![3, 1, sub, return_function, 0, sub, return_function, 0];
        let script = Arc::new(script);
        let mut vm = Vm::new_owned(Arc::clone(&script));
        vm.set_variable(Slot::Local { id: 0 }, Value::Integer(11))
            .unwrap();
        vm.step().unwrap();
        let valid = vm.checkpoint(|_| Ok("main.yks".into())).unwrap();
        let restore = |checkpoint| Vm::restore(checkpoint, |_| Ok(Arc::clone(&script)));
        let mut bad = valid.clone();
        bad.frames[0].return_pc = 0;
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.frames[0].return_target = Some(Slot::Local { id: 1 });
        assert!(restore(bad).is_err());
        let mut bad = valid.clone();
        bad.frames[0].memory.push((
            Slot::IntegerVariable {
                bank: "Flag".into(),
                id: 0,
            },
            Value::Integer(1),
        ));
        assert!(restore(bad).is_err());
        let mut restored = restore(valid).unwrap();
        restored.step().unwrap();
        assert!(matches!(
            restored.step().unwrap(),
            Event::Returned {
                destination: Some(3),
                ..
            }
        ));
        assert_eq!(
            restored.memory().get(&Slot::Local { id: 0 }),
            Some(&Value::Integer(11))
        );
    }
}
