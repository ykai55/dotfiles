pub fn subdomain_for_host(host: &str, domain: &str) -> Option<String> {
    let host_without_port = host.split(':').next().unwrap_or(host).trim_end_matches('.');
    let domain = domain.trim_end_matches('.');
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
}
