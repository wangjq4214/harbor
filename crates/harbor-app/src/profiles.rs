//! Application-owned named-launch intent. UI retains identity, never a recipe.

use std::path::PathBuf;

use harbor_config::ProfileId;

use crate::tab_manager::TabId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileLaunchIntent {
    pub profile: ProfileId,
    /// If supplied, must still be the active live source when the Host executes.
    pub source: Option<TabId>,
    /// An explicit native absolute directory; invalid values fail, never fall back.
    pub directory: Option<PathBuf>,
}
