//! Profile schema + store (Task 2). Port of `src/main/profile/{schema,store}.ts`.
pub mod schema;
pub mod store;

pub use schema::{new_id, parse_profile, parse_profile_report, settings_from_value};
pub use store::{
    ListenerId, ProfileStore, NOTICE_DROPPED, NOTICE_EXTERNAL_DAMAGED, NOTICE_FROM_BACKUP, NOTICE_FROM_STARTER,
};
