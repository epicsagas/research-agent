use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub citing_paper_id: String,
    pub cited_paper_id: String,
    pub context: String,
}

impl Citation {
    pub fn new(citing: String, cited: String) -> Self {
        Self {
            citing_paper_id: citing,
            cited_paper_id: cited,
            context: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citation_new() {
        let c = Citation::new("a".into(), "b".into());
        assert_eq!(c.citing_paper_id, "a");
        assert_eq!(c.cited_paper_id, "b");
    }
}
