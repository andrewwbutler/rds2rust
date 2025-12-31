#[cfg(target_arch = "wasm32")]
use crate::{ParseConfig, Result, RObject};

#[cfg(target_arch = "wasm32")]
use crate::wasm::{AsyncCursorConfig, AsyncRdsInput};

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct AsyncParseConfig {
    pub max_bytes: usize,
    pub cursor: AsyncCursorConfig,
}

#[cfg(target_arch = "wasm32")]
impl Default for AsyncParseConfig {
    fn default() -> Self {
        Self {
            max_bytes: 128 * 1024 * 1024,
            cursor: AsyncCursorConfig::default(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn read_rds_async(
    input: &dyn AsyncRdsInput,
    parse_config: ParseConfig,
    async_config: AsyncParseConfig,
) -> Result<RObject> {
    let config = AsyncCursorConfig {
        max_buffer_size: async_config.max_bytes,
        ..async_config.cursor
    };
    crate::parser::parse_rds_with_async_input(input, parse_config, config).await
}
