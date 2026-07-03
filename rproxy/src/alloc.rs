use rand::{distributions::Alphanumeric, Rng};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AllocError {
    #[error("invalid port range")]
    InvalidPortRange,
    #[error("port {0} is not allowed")]
    PortNotAllowed(u16),
    #[error("port {0} is unavailable")]
    PortUnavailable(u16),
    #[error("port range exhausted")]
    PortRangeExhausted,
    #[error("subdomain {0} is unavailable")]
    SubdomainUnavailable(String),
}

#[derive(Debug, Clone)]
pub struct PortAllocator {
    start: u16,
    end: u16,
    used: HashSet<u16>,
}

impl PortAllocator {
    pub fn new(start: u16, end: u16) -> Result<Self, AllocError> {
        if start > end {
            return Err(AllocError::InvalidPortRange);
        }

        Ok(Self {
            start,
            end,
            used: HashSet::new(),
        })
    }

    pub fn parse_range(input: &str) -> Result<Self, AllocError> {
        let (start, end) = input.split_once('-').ok_or(AllocError::InvalidPortRange)?;
        let start = start.parse().map_err(|_| AllocError::InvalidPortRange)?;
        let end = end.parse().map_err(|_| AllocError::InvalidPortRange)?;

        Self::new(start, end)
    }

    pub fn allocate(&mut self, requested: Option<u16>) -> Result<u16, AllocError> {
        if let Some(port) = requested {
            if port < self.start || port > self.end {
                return Err(AllocError::PortNotAllowed(port));
            }
            if !self.used.insert(port) {
                return Err(AllocError::PortUnavailable(port));
            }
            return Ok(port);
        }

        for port in self.start..=self.end {
            if self.used.insert(port) {
                return Ok(port);
            }
        }

        Err(AllocError::PortRangeExhausted)
    }

    pub fn release(&mut self, port: u16) {
        self.used.remove(&port);
    }
}

#[derive(Debug, Clone, Default)]
pub struct SubdomainAllocator {
    used: HashSet<String>,
}

impl SubdomainAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allocate(&mut self, requested: Option<&str>) -> Result<String, AllocError> {
        if let Some(subdomain) = requested {
            let subdomain = subdomain.to_string();
            if !self.used.insert(subdomain.clone()) {
                return Err(AllocError::SubdomainUnavailable(subdomain));
            }
            return Ok(subdomain);
        }

        loop {
            let random: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(6)
                .map(char::from)
                .map(|ch| ch.to_ascii_lowercase())
                .collect();
            let subdomain = format!("rp-{random}");
            if self.used.insert(subdomain.clone()) {
                return Ok(subdomain);
            }
        }
    }

    pub fn release(&mut self, subdomain: &str) {
        self.used.remove(subdomain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_requested_port_when_free_and_in_range() {
        let mut allocator = PortAllocator::new(20000, 20002).unwrap();

        assert_eq!(allocator.allocate(Some(20001)).unwrap(), 20001);
    }

    #[test]
    fn rejects_duplicate_requested_port() {
        let mut allocator = PortAllocator::new(20000, 20002).unwrap();
        allocator.allocate(Some(20001)).unwrap();

        assert_eq!(
            allocator.allocate(Some(20001)).unwrap_err(),
            AllocError::PortUnavailable(20001)
        );
    }

    #[test]
    fn rejects_requested_port_outside_range() {
        let mut allocator = PortAllocator::new(20000, 20002).unwrap();

        assert_eq!(
            allocator.allocate(Some(19999)).unwrap_err(),
            AllocError::PortNotAllowed(19999)
        );
    }

    #[test]
    fn auto_allocates_first_free_port() {
        let mut allocator = PortAllocator::new(20000, 20002).unwrap();
        allocator.allocate(Some(20000)).unwrap();

        assert_eq!(allocator.allocate(None).unwrap(), 20001);
    }

    #[test]
    fn parses_port_range() {
        let mut allocator = PortAllocator::parse_range("20000-20001").unwrap();

        assert_eq!(allocator.allocate(None).unwrap(), 20000);
        assert_eq!(allocator.allocate(None).unwrap(), 20001);
        assert_eq!(
            allocator.allocate(None).unwrap_err(),
            AllocError::PortRangeExhausted
        );
    }

    #[test]
    fn allocates_requested_subdomain_when_free() {
        let mut allocator = SubdomainAllocator::new();

        assert_eq!(allocator.allocate(Some("foo")).unwrap(), "foo");
    }

    #[test]
    fn rejects_duplicate_subdomain() {
        let mut allocator = SubdomainAllocator::new();
        allocator.allocate(Some("foo")).unwrap();

        assert_eq!(
            allocator.allocate(Some("foo")).unwrap_err(),
            AllocError::SubdomainUnavailable("foo".into())
        );
    }

    #[test]
    fn generated_subdomain_uses_expected_prefix() {
        let mut allocator = SubdomainAllocator::new();

        let subdomain = allocator.allocate(None).unwrap();

        assert!(subdomain.starts_with("rp-"));
        assert_eq!(subdomain.len(), 9);
    }
}
