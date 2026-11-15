use std::{fs, path::PathBuf};
use yuka_rs::{
    Result,
    native_save::{
        AlreadyDat, AlreadyEntry, NativeDrawable, NativeFrame, NativeLayer, NativeSave,
        NativeValue, SystemDat, read_u32, write_u32,
    },
};

#[test]
fn native_records_relocate_after_edits() -> Result<()> {
    let mut save = NativeSave::default();
    save.comment = b"slot 1".to_vec();
    save.flags[65535] = -17;
    save.strings.insert(65535, b"local text".to_vec());
    save.state_extra_strings[1] = b"voice".to_vec();
    let frame = NativeFrame {
        name: b"test".to_vec(),
        code: vec![0, 0],
        index: vec![[0, 0, 0, 0]],
        data: b"Return\0".to_vec(),
        locals: vec![NativeValue {
            kind: 2,
            bytes: b"register".to_vec(),
            ..Default::default()
        }],
        ..Default::default()
    };
    save.frames.push(frame);
    let original = save.to_bytes()?;
    let mut decoded = NativeSave::parse(&original)?;
    assert_eq!(decoded.to_bytes()?, original);
    decoded.comment.extend_from_slice(b" with a longer caption");
    let rebuilt = decoded.to_bytes()?;
    let decoded = NativeSave::parse(&rebuilt)?;
    assert_eq!(decoded.flags[65535], -17);
    assert_eq!(decoded.strings[&65535], b"local text");
    assert_eq!(decoded.frames[0].locals[0].bytes, b"register");
    assert_eq!(decoded.state_extra_strings[1], b"voice");
    assert_eq!(decoded.to_bytes()?, rebuilt);
    Ok(())
}

#[test]
fn absent_native_resources_keep_zero_offset_and_length() -> Result<()> {
    let mut save = NativeSave::default();
    save.layers.push(NativeLayer {
        items: vec![NativeDrawable {
            text: b"text".to_vec(),
            ..Default::default()
        }],
        ..Default::default()
    });
    let bytes = save.to_bytes()?;
    let state = read_u32(&bytes, 0x20)? as usize;
    for field in [0x4c, 0x50, 0x124, 0x128] {
        assert_eq!(read_u32(&bytes, state + field)?, 0);
    }
    let layer = read_u32(&bytes, state + 0x24)? as usize;
    for field in [0x5c, 0x60, 0x64, 0x68] {
        assert_eq!(read_u32(&bytes, layer + field)?, 0);
    }
    let item = read_u32(&bytes, layer + 0x6c)? as usize;
    let text = read_u32(&bytes, item + 0x54)? as usize;
    assert_eq!(read_u32(&bytes, item + 0x58)?, 5);
    assert_eq!(&bytes[text..text + 5], b"text\0");
    assert_eq!(NativeSave::parse(&bytes)?.layers[0].items[0].text, b"text");
    Ok(())
}

#[test]
fn global_and_read_marks_relocate() -> Result<()> {
    let mut system = SystemDat::default();
    system.flags[9999] = 3;
    system.strings.insert(12, b"global".to_vec());
    let mut decoded = SystemDat::parse(&system.to_bytes()?)?;
    decoded.strings.insert(2, b"added".to_vec());
    let decoded = SystemDat::parse(&decoded.to_bytes()?)?;
    assert_eq!(decoded.flags[9999], 3);
    assert_eq!(decoded.strings[&12], b"global");
    assert_eq!(decoded.strings[&2], b"added");
    let mut already = AlreadyDat::default();
    already.entries.push(AlreadyEntry {
        name: b"chapter".to_vec(),
        marks: vec![0, 1, 1],
    });
    let mut decoded = AlreadyDat::parse(&already.to_bytes()?)?;
    decoded.entries[0].name.extend_from_slice(b"_2");
    assert_eq!(
        AlreadyDat::parse(&decoded.to_bytes()?)?.entries[0].marks,
        vec![0, 1, 1]
    );
    Ok(())
}

#[test]
fn corrupt_native_offsets_and_chains_are_rejected() -> Result<()> {
    let mut save = NativeSave::default();
    save.frames = vec![NativeFrame::default(), NativeFrame::default()];
    let bytes = save.to_bytes()?;
    for cut in [0, 7, 48, bytes.len() - 1] {
        assert!(NativeSave::parse(&bytes[..cut]).is_err());
    }
    let mut corrupt = bytes.clone();
    let state = read_u32(&corrupt, 0x20)? as usize;
    write_u32(&mut corrupt, state + 0x110, u32::MAX)?;
    assert!(NativeSave::parse(&corrupt).is_err());
    let mut corrupt = bytes;
    let first = read_u32(&corrupt, state + 0x2c)?;
    write_u32(&mut corrupt, first as usize + 4, first)?;
    assert!(NativeSave::parse(&corrupt).is_err());
    Ok(())
}

#[test]
#[ignore = "requires read-only local YUKA_GAME_DIR native save samples"]
fn original_saves_roundtrip_and_rebuild() -> Result<()> {
    let game = PathBuf::from(std::env::var_os("YUKA_GAME_DIR").ok_or("set YUKA_GAME_DIR")?);
    let original = fs::read(game.join("Save/savedata.001"))?;
    let mut save = NativeSave::parse(&original)?;
    assert_eq!(save.frames.len(), 2);
    assert_eq!(save.layers.len(), 15);
    assert_eq!(save.sounds.len(), 5);
    assert_eq!(save.to_bytes()?, original);
    let mut resources =
        yuka_rs::resources::Resources::open(&game, yuka_rs::script::TextEncoding::Gbk)?;
    let mut caller = yuka_rs::vm::Vm::from_native_frame(
        &save.frames[0],
        yuka_rs::script::TextEncoding::Gbk,
        |name| resources.script(name),
        true,
    )?;
    assert!(
        matches!(caller.pending_host_call(), Some(yuka_rs::vm::Event::HostCall { function, .. }) if function == "ScriptCall")
    );
    let mut child = yuka_rs::vm::Vm::from_native_frame(
        &save.frames[1],
        yuka_rs::script::TextEncoding::Gbk,
        |name| resources.script(name),
        false,
    )?;
    assert!(
        matches!(child.step()?, yuka_rs::vm::Event::HostCall { function, .. } if function == "SaveDataSave")
    );
    child.resume_host(None)?;
    assert!(matches!(child.step()?, yuka_rs::vm::Event::Returned { .. }));
    caller.resume_script_call(None)?;
    assert_eq!(caller.pc, 0x29);
    save.comment = b"Yuka-Rs rebuilt slot".to_vec();
    save.flags[123] = 456;
    let decoded = NativeSave::parse(&save.to_bytes()?)?;
    assert_eq!(decoded.flags[123], 456);
    assert_eq!(decoded.frames[0].code, save.frames[0].code);
    assert_eq!(decoded.frames[0].data, save.frames[0].data);
    for relative in ["Save/System.dat", "全CG存档/loverable/save/System.dat"] {
        let original = fs::read(game.join(relative))?;
        let mut system = SystemDat::parse(&original)?;
        assert_eq!(system.to_bytes()?, original);
        system.flags[9999] = 123;
        let decoded = SystemDat::parse(&system.to_bytes()?)?;
        assert_eq!(decoded.flags[9999], 123);
        assert_eq!(decoded.fonts, system.fonts);
        assert_eq!(decoded.strings, system.strings);
    }
    let original = fs::read(game.join("Save/Already.dat"))?;
    let already = AlreadyDat::parse(&original)?;
    assert_eq!(already.entries.len(), 30);
    assert_eq!(already.to_bytes()?, original);
    Ok(())
}
