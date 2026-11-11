//! wasm-side tests that the async streaming traversal rejects a REFSXP whose
//! index is not in the reference table, matching the native traversal and
//! the full parser. Runs in node via `wasm-pack test --node`.

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
}
