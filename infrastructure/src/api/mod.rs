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
    AccessRequestDto, CollectionMeta, CollectionResponse, PaginationMeta, SingleResponse,
};
pub use errors::{ApiError, ProblemDetails};
pub use router::create_router;
pub use state::AppState;
