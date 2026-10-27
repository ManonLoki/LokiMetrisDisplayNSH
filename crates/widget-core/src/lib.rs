//! WidgetKit's private summary store, authenticated HTTP protocol and display projection.
//! The app and extension compile the application's same snapshot and name rules.
pub mod ffi;
pub mod store;
pub mod summary;
pub mod usage;
pub mod view;
pub mod widget_http;

#[path = "../../../src/shared/domain/agents.rs"]
pub mod agents;
#[path = "../../../src/shared/contracts/snapshot.rs"]
pub mod contracts;
#[path = "../../../src/features/connection/snapshot.rs"]
pub mod snapshot;
#[path = "../../../src/shared/domain/text.rs"]
pub mod text;

/// Canonical application source modules, shared without a second validator.
pub mod shared {
    pub mod domain {
        pub use crate::{agents, text};
    }
    pub mod contracts {
        pub use crate::contracts as snapshot;
    }
}
