pub fn subdomain_for_host(host: &str, domain: &str) -> Option<String> {
    let host_without_port = match host.split_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => host,
        Some(_) => return None,
        None => host,
    };
    let host_without_port = host_without_port.trim_end_matches('.').to_ascii_lowercase();
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    let suffix = format!(".{domain}");

    host_without_port
        .strip_suffix(&suffix)
        .filter(|subdomain| !subdomain.is_empty() && !subdomain.contains('.'))
        .map(|subdomain| subdomain.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_label_subdomain() {
        assert_eq!(subdomain_for_host("foo.a.com", "a.com"), Some("foo".into()));
    }

    #[test]
    fn ignores_port_in_host_header() {
        assert_eq!(
            subdomain_for_host("foo.a.com:8080", "a.com"),
            Some("foo".into())
        );
    }

    #[test]
    fn rejects_apex_domain() {
        assert_eq!(subdomain_for_host("a.com", "a.com"), None);
    }

    #[test]
    fn rejects_other_domain() {
        assert_eq!(subdomain_for_host("foo.other.com", "a.com"), None);
    }

    #[test]
    fn rejects_nested_subdomain() {
        assert_eq!(subdomain_for_host("bar.foo.a.com", "a.com"), None);
    }

    #[test]
    fn host_matching_is_case_insensitive() {
        assert_eq!(subdomain_for_host("Foo.A.COM", "a.com"), Some("foo".into()));
    }

    #[test]
    fn rejects_invalid_host_port() {
        assert_eq!(subdomain_for_host("foo.a.com:not-a-port", "a.com"), None);
        assert_eq!(subdomain_for_host("foo.a.com:80:garbage", "a.com"), None);
    }
}
