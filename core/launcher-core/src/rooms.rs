pub fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
            }
        })
}

pub fn valid_target(target: &str) -> bool {
    target == "lobby" || valid_id(target)
}

pub fn parse_secret(secret: &str) -> Option<String> {
    let target = secret.strip_prefix("bf1:")?;
    valid_target(target).then(|| target.to_owned())
}

pub fn parse_link(link: &str, application_id: &str) -> Option<String> {
    if let Some(target) = link.strip_prefix("blockfield://join/") {
        return valid_target(target).then(|| target.to_owned());
    }
    let url = url::Url::parse(link).ok()?;
    if url.scheme() != format!("discord-{application_id}")
        || url.host_str() != Some("join")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    let params: Vec<_> = url.query_pairs().collect();
    if params.len() != 1 || params[0].0 != "secret" {
        return None;
    }
    parse_secret(&params[0].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discord_links_decode_only_the_invite_secret() {
        let prefix = format!("discord-{APP_ID}://join?");
        assert_eq!(
            parse_link("blockfield://join/lobby", APP_ID),
            Some("lobby".into())
        );
        assert_eq!(
            parse_link(&format!("{prefix}secret=bf1%3Alobby"), APP_ID),
            Some("lobby".into())
        );
        for query in [
            "secret=bf1:lobby&host=evil",
            "secret=bf1:lobby&secret=bf1:lobby",
            "secret=bf2:lobby",
            "secret=bf1:lobby#evil",
            "secret=bf1:../../file",
        ] {
            assert_eq!(parse_link(&format!("{prefix}{query}"), APP_ID), None);
        }
    }

    #[test]
    fn links_accept_only_room_ids() {
        let id = "12345678-1234-1234-1234-123456789abc";
        assert_eq!(
            parse_link(&format!("blockfield://join/{id}"), APP_ID),
            Some(id.into())
        );
        for suffix in ["?host=evil", "/", "#fragment", "%20", "\n"] {
            assert!(parse_link(&format!("blockfield://join/{id}{suffix}"), APP_ID).is_none());
        }
        for link in [
            "https://join/1",
            "blockfield://join/1",
            "blockfield://join/../../file",
        ] {
            assert!(parse_link(link, APP_ID).is_none());
        }
    }

    /// Any valid application id exercises the same branch; the production value
    /// lives in launcher-app (`presence::APPLICATION_ID`) and is injected by callers.
    const APP_ID: &str = "1234567890123456789";
}
