use std::future::Future;

use crate::entities::system_config::SystemConfig;
use crate::errors::DomainError;

pub trait SystemConfigRepository: Send + Sync {
    fn load(&self) -> impl Future<Output = Result<SystemConfig, DomainError>> + Send;
    fn save(&self, config: &SystemConfig) -> impl Future<Output = Result<(), DomainError>> + Send;
}
