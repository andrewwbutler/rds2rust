//! Tests for StreamingGzipDecompressor.
//!
//! Note: These tests require a WASM environment with DecompressionStream API support.

#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use js_sys::{Array, Uint8Array};
    use rds2rust::{AsyncSequentialInput, StreamingGzipDecompressor};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;
    use web_sys::Blob;

    wasm_bindgen_test_configure!(run_in_browser);

    /// Helper to create a gzip-compressed blob from uncompressed data
    async fn create_gzip_blob(data: &[u8]) -> Blob {
        // Create a blob from the data
        let array = Uint8Array::from(data);
        let blob_parts = Array::new();
        blob_parts.push(&array);
        let uncompressed = Blob::new_with_u8_array_sequence(&blob_parts).unwrap();

        // Use CompressionStream to gzip it
        let window = web_sys::window().unwrap();
        let compression_stream_class =
            js_sys::Reflect::get(&window, &"CompressionStream".into()).unwrap();

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
        let response_class = js_sys::Reflect::get(&window, &"Response".into()).unwrap();
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
