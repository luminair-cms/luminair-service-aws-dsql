use async_trait::async_trait;

use crate::entities::system_config::SystemConfig;
use crate::errors::DomainError;

#[async_trait]
pub trait SystemConfigRepository: Send + Sync {
    async fn load(&self) -> Result<SystemConfig, DomainError>;
    async fn save(&self, config: &SystemConfig) -> Result<(), DomainError>;
}
