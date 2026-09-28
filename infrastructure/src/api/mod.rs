//! REST API layer adhering to `docs/api.md`.

pub mod access_requests;
pub mod documents;
pub mod dto;
pub mod errors;
pub mod health;
pub mod router;
pub mod schema;
pub mod state;

pub use dto::{
    AccessRequestDto, CollectionMeta, CollectionResponse, PaginationMeta, ParsedPayload,
    SingleResponse, parse_fields_from_json, parse_payload_from_json,
};
pub use errors::{ApiError, ProblemDetails};
pub use router::create_router;
pub use state::AppState;
