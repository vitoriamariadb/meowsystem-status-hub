pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub const UPSTREAM: &str = "https://github.com/marcelogomes90/cosmic-ext-applet-status-hub";

pub async fn open(url: &'static str) {
    match tokio::process::Command::new("xdg-open").arg(url).spawn() {
        Ok(mut handler) => {
            if let Err(error) = handler.wait().await {
                tracing::warn!(%error, url, "the link handler failed");
            }
        }
        Err(error) => tracing::warn!(%error, url, "could not reach a link handler"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_link_points_at_a_secure_github_page() {
        for url in [REPOSITORY, UPSTREAM] {
            assert!(
                url.starts_with("https://github.com/"),
                "{url} is not an https github address"
            );
        }
    }

    #[test]
    fn o_fork_credita_o_projeto_original() {
        assert_ne!(REPOSITORY, UPSTREAM);
        assert!(REPOSITORY.ends_with("/meowsystem-status-hub"));
    }
}
