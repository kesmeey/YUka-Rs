//! Bounded, native XZ decoding for the Android resource installer.
use lzma_rust2::{Action, Status, XzStream};

const INPUT_BYTES: usize = 256 * 1024;
const OUTPUT_BYTES: usize = 1024 * 1024;
const MEMORY_LIMIT_KIB: u32 = 100 * 1024;

struct Decoder {
    stream: XzStream,
    input: Vec<u8>,
    output: Vec<u8>,
    finished: bool,
}

impl Decoder {
    fn new() -> Self {
        Self {
            // Read concatenated streams and require the final EOF, including the footer/index.
            stream: XzStream::new_mem_limit(true, MEMORY_LIMIT_KIB),
            input: vec![0; INPUT_BYTES],
            output: vec![0; OUTPUT_BYTES],
            finished: false,
        }
    }

    fn process(&mut self, count: usize, eof: bool) -> std::io::Result<(usize, usize)> {
        let result = self.stream.process(
            &self.input[..count],
            &mut self.output,
            if eof { Action::Finish } else { Action::Run },
        )?;
        self.finished = result.status == Status::StreamEnd;
        Ok((result.bytes_consumed, result.bytes_produced))
    }
}

#[cfg(target_os = "android")]
mod jni_api {
    use super::*;
    use crate::Result;
    use jni::{
        JNIEnv,
        objects::{JByteArray, JClass},
        sys::{jboolean, jint, jlong},
    };
    use std::{
        collections::BTreeMap,
        sync::{Mutex, OnceLock},
    };

    #[derive(Default)]
    struct Streams {
        next: i64,
        decoders: BTreeMap<i64, Decoder>,
    }

    static STREAMS: OnceLock<Mutex<Streams>> = OnceLock::new();

    fn streams() -> &'static Mutex<Streams> {
        STREAMS.get_or_init(|| Mutex::new(Streams::default()))
    }

    fn throw(env: &mut JNIEnv<'_>, error: impl ToString) {
        let _ = env.throw_new("java/io/IOException", error.to_string());
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_yukars_player_NativeXzInputStream_create(
        mut env: JNIEnv<'_>,
        _class: JClass<'_>,
    ) -> jlong {
        let result = (|| -> Result<jlong> {
            let mut streams = streams().lock().map_err(|_| "XZ stream lock poisoned")?;
            streams.next = streams
                .next
                .checked_add(1)
                .ok_or("XZ stream identifier overflow")?;
            let handle = streams.next;
            streams.decoders.insert(handle, Decoder::new());
            Ok(handle)
        })();
        match result {
            Ok(handle) => handle,
            Err(error) => {
                throw(&mut env, error);
                0
            }
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_yukars_player_NativeXzInputStream_decode(
        mut env: JNIEnv<'_>,
        _class: JClass<'_>,
        handle: jlong,
        input: JByteArray<'_>,
        offset: jint,
        count: jint,
        output: JByteArray<'_>,
        eof: jboolean,
    ) -> jlong {
        let result = (|| -> Result<jlong> {
            let offset = usize::try_from(offset)?;
            let count = usize::try_from(count)?;
            if count > INPUT_BYTES
                || offset.checked_add(count).ok_or("XZ input overflow")?
                    > usize::try_from(env.get_array_length(&input)?)?
                || usize::try_from(env.get_array_length(&output)?)? < OUTPUT_BYTES
            {
                return Err("invalid native XZ buffer bounds".into());
            }
            let mut streams = streams().lock().map_err(|_| "XZ stream lock poisoned")?;
            let decoder = streams
                .decoders
                .get_mut(&handle)
                .ok_or("closed native XZ stream")?;
            if decoder.finished {
                return Err("native XZ stream already finished".into());
            }
            // JNI bytes are signed, but the allocated u8 storage has identical size and alignment.
            let input_bytes = unsafe {
                std::slice::from_raw_parts_mut(decoder.input.as_mut_ptr().cast::<i8>(), count)
            };
            env.get_byte_array_region(&input, i32::try_from(offset)?, input_bytes)?;
            let (consumed, produced) = decoder.process(count, eof != 0)?;
            // The slice is bounded by the decoder's initialized output buffer.
            let output_bytes = unsafe {
                std::slice::from_raw_parts(decoder.output.as_ptr().cast::<i8>(), produced)
            };
            env.set_byte_array_region(&output, 0, output_bytes)?;
            let packed = ((consumed as u64) << 32)
                | produced as u64
                | if decoder.finished { 1_u64 << 63 } else { 0 };
            Ok(packed as i64)
        })();
        match result {
            Ok(value) => value,
            Err(error) => {
                throw(&mut env, error);
                0
            }
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_org_yukars_player_NativeXzInputStream_release(
        mut env: JNIEnv<'_>,
        _class: JClass<'_>,
        handle: jlong,
    ) {
        match streams().lock() {
            Ok(mut streams) => {
                streams.decoders.remove(&handle);
            }
            Err(error) => throw(&mut env, error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let hex = "fd377a585a000004e6d6b4460200210114000000ffe7ec09e031ff004b5d002c9d49662125242a6e8a1ec19390a6cfb1c6fd5fb2f5bae08bf57074b30749a71d7db561fc5d7ddfd18a33127fdac97846eb19d507dd612dedc7065b52f9350b86bc2f8cc4082fdfa054000000a6f705ae95af67020001678064000000f93b06d3b1c467fb020000000004595a";
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect()
    }

    fn decode(bytes: &[u8], chunk: usize) -> std::io::Result<Vec<u8>> {
        let mut decoder = Decoder::new();
        let mut result = Vec::new();
        for part in bytes.chunks(chunk) {
            let mut remaining = part.len();
            decoder.input[..remaining].copy_from_slice(part);
            while remaining != 0 {
                let (consumed, produced) = decoder.process(remaining, false)?;
                assert!(consumed != 0 || produced != 0);
                result.extend_from_slice(&decoder.output[..produced]);
                decoder.input.copy_within(consumed..remaining, 0);
                remaining -= consumed;
            }
        }
        while !decoder.finished {
            let (_, produced) = decoder.process(0, true)?;
            result.extend_from_slice(&decoder.output[..produced]);
            assert!(produced != 0 || decoder.finished);
        }
        Ok(result)
    }

    #[test]
    fn native_xz_small_chunks_and_concatenated_streams() {
        let fixture = fixture();
        let expected = b"Yuka bundled archive test".repeat(512);
        assert_eq!(decode(&fixture, 1).unwrap(), expected);
        assert_eq!(
            decode(&[fixture.clone(), fixture].concat(), 17).unwrap(),
            expected.repeat(2)
        );
    }

    #[test]
    fn native_xz_requires_footer_and_checksum() {
        let mut fixture = fixture();
        assert!(decode(&fixture[..fixture.len() - 8], 17).is_err());
        fixture[110] ^= 0x80;
        assert!(decode(&fixture, INPUT_BYTES).is_err());
    }

    #[test]
    #[ignore = "requires the local bundled XZ assets"]
    fn real_bundle_decode_benchmark() -> crate::Result<()> {
        use std::{fs::File, io::Read, time::Instant};
        let bundle = std::env::var("YUKA_XZ_BENCH")?;
        let mut input = File::open(&bundle)?;
        let mut decoder = Decoder::new();
        let started = Instant::now();
        let mut output_bytes = 0_u64;
        loop {
            let count = input.read(&mut decoder.input)?;
            let eof = count == 0;
            let mut remaining = count;
            loop {
                let (consumed, produced) = decoder.process(remaining, eof)?;
                output_bytes += produced as u64;
                decoder.input.copy_within(consumed..remaining, 0);
                remaining -= consumed;
                if decoder.finished || remaining == 0 && produced == 0 {
                    break;
                }
            }
            if decoder.finished {
                break;
            }
        }
        println!(
            "native XZ: {output_bytes} bytes in {:?} ({bundle})",
            started.elapsed()
        );
        Ok(())
    }
}
