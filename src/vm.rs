//! YKS001 expressions, conditionals, and script frames with explicit host boundaries.
//! Unimplemented expressions and control flow return errors without being skipped.
use crate::Result;
use crate::script::{Element, InstructionKind, Script, Text};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Integer(i32),
    String(Text),
    Label { name: String, link: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Slot {
    IntegerVariable { bank: String, id: i32 },
    StringVariable { bank: String, id: i32 },
    Local { id: u32 },
    DataWord { offset: u32 },
    SpecialString { name: String },
}

#[derive(Debug, Clone, Serialize)]
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
        let mut vm = Vm::new_owned(Arc::new(script));
        vm.initialize_default_banks();
        assert!(matches!(vm.value(0).unwrap(), Value::Integer(0)));
        vm.set_variable(Slot::DataWord { offset: 4 }, Value::Integer(99))
            .unwrap();
        vm.step().unwrap();
        vm.step().unwrap();
        assert!(matches!(vm.step().unwrap(), Event::HostCall { .. }));
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
        let mut vm = Vm::new(&caller);
        vm.set_variable(Slot::DataWord { offset: 4 }, Value::Integer(99))
            .unwrap();
        assert!(matches!(vm.step().unwrap(), Event::Call { .. }));
        vm.step().unwrap();
        assert!(matches!(vm.step().unwrap(), Event::HostCall { .. }));
        assert!(vm.resume_script_call(None).is_err());
        vm.resume_script_jump(Arc::new(replacement)).unwrap();
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
}
