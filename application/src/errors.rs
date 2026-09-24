//! Application-level error types.

use domain::entities::role::Permission;
use domain::errors::DomainError;
use domain::value_objects::UserId;
use thiserror::Error;

/// Errors that can occur within the application layer use cases.
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// An underlying domain rule was violated.
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    /// The caller lacks the required permission to perform the action.
    #[error("unauthorized: user '{user_id}' lacks permission for action '{action:?}'")]
    Unauthorized {
        /// Identifier of the user attempting the action.
        user_id: UserId,
        /// The specific permission that was required.
        action: Permission,
    },

    /// A requested resource was not found.
    #[error("resource not found: {entity} with id '{id}'")]
    NotFound {
        /// Name of the entity type that could not be found.
        entity: &'static str,
        /// The identifier that was looked up.
        id: String,
    },

    /// An input command or query failed application-level validation.
    /// Contains all validation messages — never truncated to the first error.
    #[error("validation error: {}", .0.join("; "))]
    Validation(Vec<String>),

    /// The requested operation conflicts with existing system state.
    #[error("conflict: {0}")]
    Conflict(String),

    /// An internal system failure or unexpected error occurred.
    #[error("internal error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::value_objects::DocumentInstanceId;
    use domain::value_objects::UserId;

    #[test]
    fn test_domain_error_conversion() {
        let id = DocumentInstanceId::new(uuid::Uuid::now_v7());
        let domain_err = DomainError::DocumentInstanceNotFound(id);
        let app_err: ApplicationError = domain_err.into();

        match &app_err {
            ApplicationError::Domain(DomainError::DocumentInstanceNotFound(found_id)) => {
                assert_eq!(*found_id, id);
            }
            _ => panic!("expected ApplicationError::Domain variant"),
        }
        assert!(app_err.to_string().contains("document instance not found"));
    }

    #[test]
    fn test_unauthorized_error_display() {
        let user_id = UserId::try_new("user-123").expect("valid user id");
        let err = ApplicationError::Unauthorized {
            user_id: user_id.clone(),
            action: Permission::PublishDocument(None),
        };

        let msg = err.to_string();
        assert!(msg.contains("user 'user-123'"));
        assert!(msg.contains("PublishDocument"));
    }

    #[test]
    fn test_not_found_error_display() {
        let err = ApplicationError::NotFound {
            entity: "Role",
            id: "editor".to_string(),
        };

        assert_eq!(err.to_string(), "resource not found: Role with id 'editor'");
    }

    #[test]
    fn test_validation_error_display_single() {
        let err = ApplicationError::Validation(vec!["title must not be empty".to_string()]);
        assert_eq!(err.to_string(), "validation error: title must not be empty");
    }

    #[test]
    fn test_validation_error_display_multiple() {
        let err = ApplicationError::Validation(vec![
            "title must not be empty".to_string(),
            "body locale 'fr' unknown".to_string(),
        ]);
        let msg = err.to_string();
        assert!(msg.contains("title must not be empty"));
        assert!(msg.contains("body locale 'fr' unknown"));
    }

    #[test]
    fn test_conflict_error_display() {
        let err = ApplicationError::Conflict("locale 'en' already active".to_string());
        assert_eq!(err.to_string(), "conflict: locale 'en' already active");
    }

    #[test]
    fn test_internal_error_display() {
        let err = ApplicationError::Internal("failed to acquire lock".to_string());
        assert_eq!(err.to_string(), "internal error: failed to acquire lock");
    }
}
