use crate::config::RuntimeEnvironment;

pub const DIRTY_DEV_OVERRIDE: &str = "I_UNDERSTAND_DIRTY_DEV_BUILD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    pub package_version: &'static str,
    pub git_sha: &'static str,
    pub git_dirty: &'static str,
    pub build_timestamp: &'static str,
}

impl BuildInfo {
    pub fn current() -> Self {
        Self {
            package_version: env!("CARGO_PKG_VERSION"),
            git_sha: option_env!("POLYMARKET_GIT_SHA").unwrap_or("unknown"),
            git_dirty: option_env!("POLYMARKET_GIT_DIRTY").unwrap_or("unknown"),
            build_timestamp: option_env!("POLYMARKET_BUILD_TIMESTAMP").unwrap_or("unknown"),
        }
    }

    pub fn git_short_sha(&self) -> &str {
        if self.git_sha.len() >= 12 {
            &self.git_sha[..12]
        } else {
            self.git_sha
        }
    }

    pub fn is_git_sha_known(&self) -> bool {
        self.git_sha != "unknown" && self.git_sha.len() >= 7
    }

    pub fn is_dirty_known(&self) -> bool {
        matches!(self.git_dirty, "true" | "false")
    }

    pub fn is_dirty(&self) -> bool {
        self.git_dirty == "true"
    }
}

pub fn live_provenance_rejection(
    info: &BuildInfo,
    environment: RuntimeEnvironment,
    dirty_dev_override: bool,
) -> Option<String> {
    if !info.is_git_sha_known() {
        return Some("build git revision is unknown".to_string());
    }
    if !info.is_dirty_known() {
        return Some("build dirty-tree status is unknown".to_string());
    }
    if info.is_dirty() && !(environment == RuntimeEnvironment::Development && dirty_dev_override) {
        return Some(format!(
            "build {} was produced from a dirty worktree",
            info.git_short_sha()
        ));
    }
    None
}

pub fn dirty_dev_override_enabled() -> bool {
    std::env::var("POLYMARKET_ALLOW_DIRTY_DEV_BUILD")
        .map(|value| value == DIRTY_DEV_OVERRIDE)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(git_sha: &'static str, git_dirty: &'static str) -> BuildInfo {
        BuildInfo {
            package_version: "0.1.0",
            git_sha,
            git_dirty,
            build_timestamp: "123",
        }
    }

    #[test]
    fn rejects_unknown_live_provenance() {
        let rejection = live_provenance_rejection(
            &info("unknown", "false"),
            RuntimeEnvironment::Production,
            false,
        );
        assert_eq!(rejection, Some("build git revision is unknown".to_string()));

        let rejection = live_provenance_rejection(
            &info("abcdef123456", "unknown"),
            RuntimeEnvironment::Production,
            false,
        );
        assert_eq!(
            rejection,
            Some("build dirty-tree status is unknown".to_string())
        );
    }

    #[test]
    fn rejects_dirty_production_builds_even_with_dev_override() {
        let rejection = live_provenance_rejection(
            &info("abcdef123456", "true"),
            RuntimeEnvironment::Production,
            true,
        );
        assert_eq!(
            rejection,
            Some("build abcdef123456 was produced from a dirty worktree".to_string())
        );
    }

    #[test]
    fn permits_dirty_builds_only_for_explicit_development_override() {
        assert!(live_provenance_rejection(
            &info("abcdef123456", "true"),
            RuntimeEnvironment::Development,
            false,
        )
        .is_some());
        assert!(live_provenance_rejection(
            &info("abcdef123456", "true"),
            RuntimeEnvironment::Development,
            true,
        )
        .is_none());
    }

    #[test]
    fn accepts_clean_known_provenance() {
        assert!(live_provenance_rejection(
            &info("abcdef123456", "false"),
            RuntimeEnvironment::Production,
            false,
        )
        .is_none());
    }
}
