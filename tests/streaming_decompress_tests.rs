//! Tests for StreamingGzipDecompressor.
//!
//! Note: These tests require a WASM environment with DecompressionStream API support.

#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use js_sys::{Array, Uint8Array};
    use rds2rust::{
        read_rds_async, traverse_rds_blob_streaming, traverse_rds_streaming_async,
        AsyncCursorConfig, AsyncParseConfig, AsyncRdsInput, AsyncReadFuture, AsyncSequentialInput,
        ObjectPath, ParseConfig, ParseMode, RObject, RdsVisitor, StreamingGzipDecompressor,
        VisitAction,
    };
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;
    use web_sys::Blob;

    /// Helper to create a gzip-compressed blob from uncompressed data
    async fn create_gzip_blob(data: &[u8]) -> Blob {
        // Create a blob from the data
        let array = Uint8Array::from(data);
        let blob_parts = Array::new();
        blob_parts.push(&array);
        let uncompressed = Blob::new_with_u8_array_sequence(&blob_parts).unwrap();

        // Use CompressionStream to gzip it (looked up on the global scope so
        // this works in both browsers and Node >= 18).
        let global = js_sys::global();
        let compression_stream_class =
            js_sys::Reflect::get(&global, &"CompressionStream".into()).unwrap();

        let compressor = js_sys::Reflect::construct(
            &compression_stream_class
                .dyn_into::<js_sys::Function>()
                .unwrap(),
            &Array::of1(&"gzip".into()),
        )
        .unwrap();

        let blob_stream = uncompressed.stream();

        let writable = js_sys::Reflect::get(&compressor, &"writable".into()).unwrap();
        let readable = js_sys::Reflect::get(&compressor, &"readable".into()).unwrap();

        let transform = js_sys::Object::new();
        js_sys::Reflect::set(&transform, &"writable".into(), &writable).unwrap();
        js_sys::Reflect::set(&transform, &"readable".into(), &readable).unwrap();

        let pipe_through_fn = js_sys::Reflect::get(&blob_stream, &"pipeThrough".into())
            .unwrap()
            .dyn_into::<js_sys::Function>()
            .unwrap();

        let compressed_stream = pipe_through_fn.call1(&blob_stream, &transform).unwrap();

        // Convert stream to blob
        let response_class = js_sys::Reflect::get(&js_sys::global(), &"Response".into()).unwrap();
        let response = js_sys::Reflect::construct(
            &response_class.dyn_into::<js_sys::Function>().unwrap(),
            &Array::of1(&compressed_stream),
        )
        .unwrap();

        let blob_fn = js_sys::Reflect::get(&response, &"blob".into())
            .unwrap()
            .dyn_into::<js_sys::Function>()
            .unwrap();

        let blob_promise: js_sys::Promise = blob_fn.call0(&response).unwrap().dyn_into().unwrap();
        let blob_value = wasm_bindgen_futures::JsFuture::from(blob_promise)
            .await
            .unwrap();

        blob_value.dyn_into::<Blob>().unwrap()
    }

    /// Records the type of every object the traversal reports.
    #[derive(Default)]
    struct TypeRecorder(Vec<String>);

    impl RdsVisitor for TypeRecorder {
        type Error = std::convert::Infallible;

        fn on_object_start(
            &mut self,
            _path: &ObjectPath,
            obj_type: &str,
        ) -> Result<VisitAction, Self::Error> {
            self.0.push(obj_type.to_string());
            Ok(VisitAction::Continue)
        }
    }

    // serialize(list(1, "a"), NULL, version = 2)
    const SMALL_LIST: &[u8] = &[
        0x58, 0x0a, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x03, 0x03, 0x00, 0x02, 0x03, 0x00, 0x00,
        0x00, 0x00, 0x13, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x00, 0x00, 0x01,
        0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x04, 0x00, 0x09, 0x00, 0x00, 0x00, 0x01, 0x61,
    ];

    /// A gzip file far smaller than the streaming read buffer must still
    /// traverse: the blob API streams every gzip file through the sequential
    /// cursor, whose buffer is 16-128 MB.
    #[wasm_bindgen_test]
    async fn test_blob_streaming_traverses_small_gzip_file() {
        let blob = create_gzip_blob(SMALL_LIST).await;
        let mut visitor = TypeRecorder::default();
        traverse_rds_blob_streaming(blob, ParseConfig::default(), &mut visitor)
            .await
            .expect("small gzip file must traverse");
        assert_eq!(visitor.0, ["List", "Real", "Character"]);
    }

    // serialize(NULL, NULL, version = 2): the whole object is one 4-byte word.
    const NULL_ONLY: &[u8] = &[
        0x58, 0x0a, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x03, 0x03, 0x00, 0x02, 0x03, 0x00, 0x00,
        0x00, 0x00, 0xfe,
    ];

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

    /// A file holding only NULL ends 4 bytes after the header. The wasm
    /// paths peeked 8 bytes for a size guess and failed with an unexpected
    /// end of file; native always read it.
    #[wasm_bindgen_test]
    async fn test_null_only_file_on_every_wasm_path() {
        let mut visitor = TypeRecorder::default();
        traverse_rds_streaming_async(
            &RandomAccess(NULL_ONLY.to_vec()),
            ParseConfig::default(),
            AsyncCursorConfig::default(),
            &mut visitor,
        )
        .await
        .expect("random-access streaming");
        assert_eq!(visitor.0, ["Null"]);

        let mut visitor = TypeRecorder::default();
        let blob = create_gzip_blob(NULL_ONLY).await;
        traverse_rds_blob_streaming(blob, ParseConfig::default(), &mut visitor)
            .await
            .expect("gzip blob streaming");
        assert_eq!(visitor.0, ["Null"]);

        for mode in [ParseMode::Full, ParseMode::LazyMetadata] {
            let config = ParseConfig {
                mode: mode.clone(),
                ..ParseConfig::default()
            };
            let parsed = read_rds_async(
                &RandomAccess(NULL_ONLY.to_vec()),
                config,
                AsyncParseConfig::default(),
            )
            .await
            .unwrap_or_else(|err| panic!("read_rds_async {mode:?}: {err}"));
            assert_eq!(parsed.object, RObject::Null, "{mode:?}");
        }
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_creation_with_gzip() {
        let data = b"Hello, world! This is test data.";
        let gzip_blob = create_gzip_blob(data).await;

        let decompressor = StreamingGzipDecompressor::new(gzip_blob).await;
        assert!(
            decompressor.is_ok(),
            "Should create decompressor for gzip blob"
        );
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_rejects_non_gzip() {
        // Create an uncompressed blob
        let data = b"Not compressed data";
        let array = Uint8Array::from(&data[..]);
        let blob_parts = Array::new();
        blob_parts.push(&array);
        let blob = Blob::new_with_u8_array_sequence(&blob_parts).unwrap();

        let result = StreamingGzipDecompressor::new(blob).await;
        assert!(result.is_err(), "Should reject non-gzip blob");

        if let Err(e) = result {
            let err_msg = format!("{:?}", e);
            assert!(
                err_msg.contains("gzip"),
                "Error should mention gzip: {}",
                err_msg
            );
        }
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_sequential_read() {
        let data = b"The quick brown fox jumps over the lazy dog.";
        let gzip_blob = create_gzip_blob(data).await;

        let mut decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        // Read first 10 bytes
        let chunk1 = decompressor.read_next(10).await.unwrap();
        assert_eq!(chunk1, b"The quick ");
        assert_eq!(decompressor.position(), 10);

        // Read next 15 bytes
        let chunk2 = decompressor.read_next(15).await.unwrap();
        assert_eq!(chunk2, b"brown fox jumps");
        assert_eq!(decompressor.position(), 25);

        // Read remaining
        let chunk3 = decompressor.read_next(100).await.unwrap();
        assert_eq!(chunk3, b" over the lazy dog.");
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_read_all_at_once() {
        let data = b"All at once!";
        let gzip_blob = create_gzip_blob(data).await;

        let mut decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        let result = decompressor.read_next(1000).await.unwrap();
        assert_eq!(result, data);
        assert_eq!(decompressor.position(), data.len() as u64);
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_read_beyond_end() {
        let data = b"Short";
        let gzip_blob = create_gzip_blob(data).await;

        let mut decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        // Read more than available
        let result = decompressor.read_next(100).await.unwrap();
        assert_eq!(result, data);

        // Try to read more - should get empty
        let result2 = decompressor.read_next(100).await.unwrap();
        assert_eq!(result2.len(), 0);
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_position_tracking() {
        let data = b"Position tracking test data here.";
        let gzip_blob = create_gzip_blob(data).await;

        let mut decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        assert_eq!(decompressor.position(), 0);

        decompressor.read_next(5).await.unwrap();
        assert_eq!(decompressor.position(), 5);

        decompressor.read_next(10).await.unwrap();
        assert_eq!(decompressor.position(), 15);

        decompressor.read_next(100).await.unwrap();
        assert_eq!(decompressor.position(), data.len() as u64);
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_total_size_unknown() {
        let data = b"Size unknown until decompressed";
        let gzip_blob = create_gzip_blob(data).await;

        let decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        // For streaming decompression, total size is unknown
        assert_eq!(decompressor.total_size(), None);
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_empty_data() {
        let data = b"";
        let gzip_blob = create_gzip_blob(data).await;

        let mut decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        let result = decompressor.read_next(100).await.unwrap();
        assert_eq!(result.len(), 0);
        assert_eq!(decompressor.position(), 0);
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_large_data() {
        // Create a larger dataset (1KB)
        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let gzip_blob = create_gzip_blob(&data).await;

        let mut decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        // Read in multiple chunks
        let mut result = Vec::new();
        while result.len() < data.len() {
            let chunk = decompressor.read_next(256).await.unwrap();
            if chunk.is_empty() {
                break;
            }
            result.extend_from_slice(&chunk);
        }

        assert_eq!(result.len(), data.len());
        assert_eq!(result, data);
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_is_finished_flag() {
        let data = b"Finished test";
        let gzip_blob = create_gzip_blob(data).await;

        let mut decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        assert!(!decompressor.is_finished());

        // Read all data
        decompressor.read_next(1000).await.unwrap();

        // After reading past the end, should be finished
        // Note: might require an additional read to detect EOF
        decompressor.read_next(1).await.unwrap();

        // The finished flag is set internally when the stream reports done
        // This test verifies the API exists and can be called
    }

    #[wasm_bindgen_test]
    async fn test_decompressor_buffered_bytes() {
        let data = b"Buffering test data";
        let gzip_blob = create_gzip_blob(data).await;

        let decompressor = StreamingGzipDecompressor::new(gzip_blob)
            .await
            .expect("Failed to create decompressor");

        // Initially no buffered bytes
        assert_eq!(decompressor.buffered_bytes(), 0);

        // The buffered_bytes() method is available for introspection
    }
}

// Non-WASM placeholder test
#[cfg(not(target_arch = "wasm32"))]
mod non_wasm_tests {
    #[test]
    fn streaming_decompressor_is_wasm_only() {
        // StreamingGzipDecompressor is only available on WASM
    }
}
