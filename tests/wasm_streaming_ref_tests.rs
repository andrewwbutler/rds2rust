//! wasm-side tests for the async streaming traversal: it rejects a REFSXP
//! whose index is not in the reference table, matching the native traversal
//! and the full parser, and it delivers each event once even when it has to
//! retry an object with a larger window. Runs in node via
//! `wasm-pack test --node`.

#[cfg(target_arch = "wasm32")]
mod wasm_streaming_ref_tests {
    use rds2rust::{
        traverse_rds_streaming_async, AsyncCursorConfig, AsyncRdsInput, AsyncReadFuture,
        ObjectPath, ParseConfig, RdsVisitor, VisitAction,
    };
    use wasm_bindgen_test::*;

    struct RandomAccess(Vec<u8>);

    impl AsyncRdsInput for RandomAccess {
        fn read_at<'a>(&'a self, offset: u64, len: usize) -> AsyncReadFuture<'a> {
            Box::pin(async move {
                let start = (offset as usize).min(self.0.len());
                let end = start.saturating_add(len).min(self.0.len());
                Ok(self.0[start..end].to_vec())
            })
        }

        fn len(&self) -> Option<u64> {
            Some(self.0.len() as u64)
        }
    }

    struct Visitor;

    impl RdsVisitor for Visitor {
        type Error = std::convert::Infallible;

        fn on_object_start(
            &mut self,
            _path: &ObjectPath,
            _obj_type: &str,
        ) -> Result<VisitAction, Self::Error> {
            Ok(VisitAction::Continue)
        }
    }

    fn words(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_be_bytes()).collect()
    }

    /// Version-2 stream for pairlist(a = NULL) whose tail is <REFSXP `tail`>.
    /// `a` is the only object with a reference index (1). CHARSXP bytes are
    /// unpadded, so 'a' is a single byte.
    fn pairlist_with_tail_ref(tail: u32) -> Vec<u8> {
        let mut bytes = b"X\n".to_vec();
        bytes.extend(words(&[
            2,
            0x0004_0303,
            0x0002_0300,
            0x402,
            0x01,
            0x0004_0009,
            1,
        ]));
        bytes.push(b'a');
        bytes.extend(words(&[0xfe, (tail << 8) | 0xff]));
        bytes
    }

    async fn traverse_random_access(data: Vec<u8>) -> Result<(), String> {
        traverse_rds_streaming_async(
            &RandomAccess(data),
            ParseConfig::default(),
            AsyncCursorConfig::default(),
            &mut Visitor,
        )
        .await
        .map_err(|err| err.to_string())
    }

    #[wasm_bindgen_test]
    async fn test_random_access_rejects_out_of_range_tail_reference() {
        let err = traverse_random_access(pairlist_with_tail_ref(5))
            .await
            .expect_err("dangling tail reference must fail");
        assert!(err.contains("Invalid reference index: 5"), "{err}");
        traverse_random_access(pairlist_with_tail_ref(1))
            .await
            .expect("valid tail back-reference must traverse");
    }

    /// Records every callback as "type path" (or "end path"), with paths
    /// joined by '/'.
    #[derive(Default)]
    struct EventRecorder(Vec<String>);

    impl RdsVisitor for EventRecorder {
        type Error = std::convert::Infallible;

        fn on_object_start(
            &mut self,
            path: &ObjectPath,
            obj_type: &str,
        ) -> Result<VisitAction, Self::Error> {
            self.0
                .push(format!("{obj_type} {}", path.segments.join("/")));
            Ok(VisitAction::Continue)
        }

        fn on_object_end(&mut self, path: &ObjectPath) -> Result<(), Self::Error> {
            self.0.push(format!("end {}", path.segments.join("/")));
            Ok(())
        }
    }

    /// list(<1 MiB raw vector>, 1.5, "end"): far bigger than the first
    /// window the traversal tries, so it retries with larger ones.
    fn list_with_large_raw() -> Vec<u8> {
        let n = 1024 * 1024;
        let mut bytes = b"X\n".to_vec();
        bytes.extend(words(&[2, 0x0004_0303, 0x0002_0300, 0x13, 3, 0x18, n]));
        bytes.resize(bytes.len() + n as usize, 0);
        bytes.extend(words(&[0x0e, 1]));
        bytes.extend(1.5_f64.to_be_bytes());
        bytes.extend(words(&[0x10, 1, 0x0004_0009, 3]));
        bytes.extend(b"end");
        bytes
    }

    /// Each retry used to deliver the events of the failed attempts again,
    /// and to start from a path left deeper by the failed attempt.
    #[wasm_bindgen_test]
    async fn test_retries_deliver_each_event_once() {
        let mut visitor = EventRecorder::default();
        traverse_rds_streaming_async(
            &RandomAccess(list_with_large_raw()),
            ParseConfig::default(),
            AsyncCursorConfig::default(),
            &mut visitor,
        )
        .await
        .expect("traversal must succeed");
        let starts: Vec<&str> = visitor
            .0
            .iter()
            .filter(|event| !event.starts_with("end "))
            .map(String::as_str)
            .collect();
        assert_eq!(starts, ["List ", "Raw [0]", "Real [1]", "Character [2]"]);
        let ends = visitor.0.len() - starts.len();
        assert_eq!(ends, 4, "one end per object: {:?}", visitor.0);
    }

    /// A visitor that skips the large element must be asked once, and its
    /// answer must still hold on the retries.
    #[wasm_bindgen_test]
    async fn test_retries_reuse_visitor_decisions() {
        #[derive(Default)]
        struct SkipFirstChild {
            starts: usize,
        }

        impl RdsVisitor for SkipFirstChild {
            type Error = std::convert::Infallible;

            fn on_object_start(
                &mut self,
                path: &ObjectPath,
                _obj_type: &str,
            ) -> Result<VisitAction, Self::Error> {
                self.starts += 1;
                if path.segments.len() == 1 && path.segments[0].as_ref() == "[0]" {
                    Ok(VisitAction::Skip)
                } else {
                    Ok(VisitAction::Continue)
                }
            }
        }

        let mut visitor = SkipFirstChild::default();
        traverse_rds_streaming_async(
            &RandomAccess(list_with_large_raw()),
            ParseConfig::default(),
            AsyncCursorConfig::default(),
            &mut visitor,
        )
        .await
        .expect("traversal must succeed");
        assert_eq!(visitor.starts, 4);
    }
}
