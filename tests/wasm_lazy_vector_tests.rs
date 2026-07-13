#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use std::sync::Arc;

    use rds2rust::constants::{CHARSXP, REFSXP};
    use rds2rust::{
        read_lazy_character_range_async, read_lazy_complex_range_async,
        read_lazy_integer_range_async, read_lazy_logical_range_async, read_lazy_raw_range_async,
        read_lazy_real_range_async, AsyncRdsInput, AsyncReadFuture, Complex, LazyVector, Logical,
    };
    use wasm_bindgen_test::*;

    struct TestAsyncInput {
        data: Vec<u8>,
    }

    impl TestAsyncInput {
        fn new(data: Vec<u8>) -> Self {
            Self { data }
        }
    }

    impl AsyncRdsInput for TestAsyncInput {
        fn read_at<'a>(&'a self, offset: u64, len: usize) -> AsyncReadFuture<'a> {
            Box::pin(async move {
                let start = offset as usize;
                let end = start.saturating_add(len);
                if end > self.data.len() {
                    return Err(rds2rust::Error::UnexpectedEofDetail {
                        position: start,
                        needed: len,
                        available: self.data.len().saturating_sub(start),
                    });
                }
                Ok(self.data[start..end].to_vec())
            })
        }

        fn len(&self) -> Option<u64> {
            Some(self.data.len() as u64)
        }
    }

    fn span_for(len: usize, elem_bytes: usize) -> LazyVector {
        LazyVector {
            length: len,
            offset: 0,
            byte_len: (len * elem_bytes) as u64,
        }
    }

    #[wasm_bindgen_test]
    async fn read_lazy_integer_range_async_reads_slice() {
        let values = [10i32, 20, 30, 40, 50];
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let input = TestAsyncInput::new(data);
        let span = span_for(5, 4);
        let out = read_lazy_integer_range_async(&input, span, 1, 3)
            .await
            .unwrap();
        assert_eq!(out, vec![20, 30, 40]);
    }

    #[wasm_bindgen_test]
    async fn read_lazy_real_range_async_reads_slice() {
        let values = [1.5f64, 2.5, 3.5, 4.5];
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let input = TestAsyncInput::new(data);
        let span = span_for(4, 8);
        let out = read_lazy_real_range_async(&input, span, 0, 2)
            .await
            .unwrap();
        assert_eq!(out, vec![1.5, 2.5]);
    }

    #[wasm_bindgen_test]
    async fn read_lazy_logical_range_async_reads_slice() {
        let values = [1i32, 0, i32::MIN, 5];
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let input = TestAsyncInput::new(data);
        let span = span_for(4, 4);
        let out = read_lazy_logical_range_async(&input, span, 0, 4)
            .await
            .unwrap();
        assert_eq!(
            out,
            vec![Logical::True, Logical::False, Logical::Na, Logical::Na]
        );
    }

    #[wasm_bindgen_test]
    async fn read_lazy_raw_range_async_reads_slice() {
        let data = vec![1u8, 2, 3, 4, 5];
        let input = TestAsyncInput::new(data);
        let span = span_for(5, 1);
        let out = read_lazy_raw_range_async(&input, span, 2, 2).await.unwrap();
        assert_eq!(out, vec![3, 4]);
    }

    #[wasm_bindgen_test]
    async fn read_lazy_complex_range_async_reads_slice() {
        let values = [
            Complex {
                real: 1.0,
                imaginary: -1.0,
            },
            Complex {
                real: 2.5,
                imaginary: 3.5,
            },
            Complex {
                real: -4.0,
                imaginary: 0.25,
            },
        ];
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(&value.real.to_be_bytes());
            data.extend_from_slice(&value.imaginary.to_be_bytes());
        }
        let input = TestAsyncInput::new(data);
        let span = span_for(3, 16);
        let out = read_lazy_complex_range_async(&input, span, 1, 2)
            .await
            .unwrap();
        assert_eq!(
            out,
            vec![
                Complex {
                    real: 2.5,
                    imaginary: 3.5
                },
                Complex {
                    real: -4.0,
                    imaginary: 0.25
                }
            ]
        );
    }

    #[wasm_bindgen_test]
    async fn read_lazy_character_range_async_reads_slice() {
        let mut data = Vec::new();
        data.extend_from_slice(&CHARSXP.to_be_bytes());
        data.extend_from_slice(&(1i32).to_be_bytes());
        data.extend_from_slice(b"a");
        data.extend_from_slice(&CHARSXP.to_be_bytes());
        data.extend_from_slice(&(3i32).to_be_bytes());
        data.extend_from_slice(b"bbb");
        let ref_flags = (2u32 << 8) | REFSXP;
        data.extend_from_slice(&ref_flags.to_be_bytes());

        let input = TestAsyncInput::new(data);
        let span = LazyVector {
            length: 3,
            offset: 0,
            byte_len: input.data.len() as u64,
        };
        let out = read_lazy_character_range_async(&input, span, 1, 2)
            .await
            .unwrap();
        assert_eq!(out, vec![Some(Arc::from("bbb")), Some(Arc::from("bbb"))]);
    }
}
