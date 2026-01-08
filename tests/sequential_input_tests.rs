//! Tests for AsyncSequentialInput trait and SequentialAdapter.

#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use rds2rust::{
        AsyncBufferedCursor, AsyncCursor, AsyncCursorConfig, AsyncRdsInput, AsyncReadFuture,
        AsyncSequentialInput, SequentialAdapter, SequentialCursor,
    };
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    /// Mock random-access input source for testing
    struct MockRdsInput {
        data: Vec<u8>,
    }

    impl MockRdsInput {
        fn new(data: Vec<u8>) -> Self {
            Self { data }
        }
    }

    impl AsyncRdsInput for MockRdsInput {
        fn read_at<'a>(&'a self, offset: u64, len: usize) -> AsyncReadFuture<'a> {
            Box::pin(async move {
                let start = offset as usize;
                let end = (start + len).min(self.data.len());

                if start >= self.data.len() {
                    return Ok(Vec::new());
                }

                Ok(self.data[start..end].to_vec())
            })
        }

        fn len(&self) -> Option<u64> {
            Some(self.data.len() as u64)
        }
    }

    /// Mock sequential input for testing the trait directly
    struct MockSequentialInput {
        data: Vec<u8>,
        position: usize,
    }

    impl MockSequentialInput {
        fn new(data: Vec<u8>) -> Self {
            Self { data, position: 0 }
        }
    }

    impl AsyncSequentialInput for MockSequentialInput {
        fn read_next<'a>(&'a mut self, len: usize) -> AsyncReadFuture<'a> {
            Box::pin(async move {
                let start = self.position;
                let end = (start + len).min(self.data.len());

                if start >= self.data.len() {
                    return Ok(Vec::new());
                }

                let result = self.data[start..end].to_vec();
                self.position = end;
                Ok(result)
            })
        }

        fn total_size(&self) -> Option<u64> {
            Some(self.data.len() as u64)
        }

        fn position(&self) -> u64 {
            self.position as u64
        }
    }

    async fn read_u32_generic<C: AsyncCursor>(cursor: &mut C) -> u32 {
        cursor.ensure_available(4).await.unwrap();
        let slice = cursor.as_sync_slice(4).unwrap();
        let value = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]);
        cursor.advance(4).unwrap();
        value
    }

    #[wasm_bindgen_test]
    async fn test_sequential_adapter_forward_reading() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let input = MockRdsInput::new(data.clone());
        let mut adapter = SequentialAdapter::new(input);

        // Read first 3 bytes
        let chunk1 = adapter.read_next(3).await.unwrap();
        assert_eq!(chunk1, vec![1, 2, 3]);
        assert_eq!(adapter.position(), 3);

        // Read next 4 bytes
        let chunk2 = adapter.read_next(4).await.unwrap();
        assert_eq!(chunk2, vec![4, 5, 6, 7]);
        assert_eq!(adapter.position(), 7);

        // Read remaining bytes
        let chunk3 = adapter.read_next(10).await.unwrap();
        assert_eq!(chunk3, vec![8, 9, 10]);
        assert_eq!(adapter.position(), 10);
    }

    #[wasm_bindgen_test]
    async fn test_sequential_adapter_total_size() {
        let data = vec![1, 2, 3, 4, 5];
        let input = MockRdsInput::new(data.clone());
        let adapter = SequentialAdapter::new(input);

        assert_eq!(adapter.total_size(), Some(5));
    }

    #[wasm_bindgen_test]
    async fn test_sequential_adapter_position_tracking() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let input = MockRdsInput::new(data.clone());
        let mut adapter = SequentialAdapter::new(input);

        assert_eq!(adapter.position(), 0);

        adapter.read_next(2).await.unwrap();
        assert_eq!(adapter.position(), 2);

        adapter.read_next(3).await.unwrap();
        assert_eq!(adapter.position(), 5);

        adapter.read_next(100).await.unwrap(); // Read beyond end
        assert_eq!(adapter.position(), 8);
    }

    #[wasm_bindgen_test]
    async fn test_sequential_adapter_empty_input() {
        let data = vec![];
        let input = MockRdsInput::new(data);
        let mut adapter = SequentialAdapter::new(input);

        let chunk = adapter.read_next(10).await.unwrap();
        assert_eq!(chunk, Vec::<u8>::new());
        assert_eq!(adapter.position(), 0);
    }

    #[wasm_bindgen_test]
    async fn test_sequential_adapter_read_beyond_end() {
        let data = vec![1, 2, 3];
        let input = MockRdsInput::new(data);
        let mut adapter = SequentialAdapter::new(input);

        // Read all data
        let chunk1 = adapter.read_next(3).await.unwrap();
        assert_eq!(chunk1, vec![1, 2, 3]);

        // Try to read more
        let chunk2 = adapter.read_next(5).await.unwrap();
        assert_eq!(chunk2, Vec::<u8>::new());
    }

    #[wasm_bindgen_test]
    async fn test_mock_sequential_input_forward_only() {
        let data = vec![10, 20, 30, 40, 50];
        let mut input = MockSequentialInput::new(data);

        assert_eq!(input.position(), 0);
        assert_eq!(input.total_size(), Some(5));

        let chunk1 = input.read_next(2).await.unwrap();
        assert_eq!(chunk1, vec![10, 20]);
        assert_eq!(input.position(), 2);

        let chunk2 = input.read_next(2).await.unwrap();
        assert_eq!(chunk2, vec![30, 40]);
        assert_eq!(input.position(), 4);

        let chunk3 = input.read_next(5).await.unwrap();
        assert_eq!(chunk3, vec![50]);
        assert_eq!(input.position(), 5);
    }

    #[wasm_bindgen_test]
    async fn test_sequential_adapter_inner_access() {
        let data = vec![1, 2, 3, 4, 5];
        let input = MockRdsInput::new(data.clone());
        let adapter = SequentialAdapter::new(input);

        // Verify we can access the inner source
        assert_eq!(adapter.inner().len(), Some(5));
    }

    #[wasm_bindgen_test]
    async fn test_sequential_reads_single_bytes() {
        let data = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let input = MockRdsInput::new(data);
        let mut adapter = SequentialAdapter::new(input);

        // Read one byte at a time
        for expected in [0xAA, 0xBB, 0xCC, 0xDD] {
            let chunk = adapter.read_next(1).await.unwrap();
            assert_eq!(chunk.len(), 1);
            assert_eq!(chunk[0], expected);
        }

        assert_eq!(adapter.position(), 4);
    }

    #[wasm_bindgen_test]
    async fn test_sequential_read_zero_bytes() {
        let data = vec![1, 2, 3];
        let input = MockRdsInput::new(data);
        let mut adapter = SequentialAdapter::new(input);

        let chunk = adapter.read_next(0).await.unwrap();
        assert_eq!(chunk, Vec::<u8>::new());
        assert_eq!(adapter.position(), 0);

        // Position should not advance
        let chunk2 = adapter.read_next(2).await.unwrap();
        assert_eq!(chunk2, vec![1, 2]);
    }

    #[wasm_bindgen_test]
    async fn test_sequential_large_read() {
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let input = MockRdsInput::new(data.clone());
        let mut adapter = SequentialAdapter::new(input);

        let chunk = adapter.read_next(1000).await.unwrap();
        assert_eq!(chunk.len(), 1000);
        assert_eq!(chunk, data);
        assert_eq!(adapter.position(), 1000);
    }

    #[wasm_bindgen_test]
    async fn test_async_cursor_with_sequential_cursor() {
        let data = vec![0, 0, 0, 5, 10, 11, 12, 13];
        let mut input = MockSequentialInput::new(data);
        let config = AsyncCursorConfig {
            buffer_size: 4,
            max_buffer_size: 8,
        };
        let mut cursor = SequentialCursor::with_config(&mut input, config)
            .await
            .unwrap();

        let first = read_u32_generic(&mut cursor).await;
        assert_eq!(first, 5);

        let slice = cursor.as_sync_slice(4).unwrap();
        assert_eq!(slice, &[10, 11, 12, 13]);
    }

    #[wasm_bindgen_test]
    async fn test_async_cursor_with_buffered_cursor() {
        let data = vec![0, 0, 0, 7, 20, 21, 22, 23];
        let input = MockRdsInput::new(data);
        let config = AsyncCursorConfig {
            buffer_size: 4,
            max_buffer_size: 8,
        };
        let mut cursor = AsyncBufferedCursor::new(&input, config).await.unwrap();

        let first = read_u32_generic(&mut cursor).await;
        assert_eq!(first, 7);

        let slice = cursor.as_sync_slice(4).unwrap();
        assert_eq!(slice, &[20, 21, 22, 23]);
    }
}

// Non-WASM tests can go here if needed
#[cfg(not(target_arch = "wasm32"))]
mod non_wasm_tests {
    // These traits are WASM-only, so we just add a placeholder test
    #[test]
    fn sequential_input_is_wasm_only() {
        // The sequential input traits are only available on WASM
        // This test just ensures the file compiles on non-WASM targets
        assert!(true);
    }
}
