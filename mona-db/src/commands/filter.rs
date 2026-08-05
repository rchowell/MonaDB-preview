use bson::{Bson, Document};

use crate::predicate::Predicate;
use crate::error::Result;
use crate::storage::CollectionRegistry;

/// Supported write/find query shapes with optional `_id` point-get fast path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QueryFilter {
    All,
    ById(Bson),
    Expr(Predicate),
}

impl QueryFilter {
    pub(crate) fn from_query(query: &Document) -> Result<Self> {
        let pred = Predicate::parse(query)?;
        if matches!(pred, Predicate::Always) {
            return Ok(Self::All);
        }
        if let Some(id) = pred.as_id_eq() {
            return Ok(Self::ById(id.clone()));
        }
        Ok(Self::Expr(pred))
    }

    /// Predicate applied during scans (`None` means match all).
    pub(crate) fn predicate(&self) -> Option<&Predicate> {
        match self {
            Self::All | Self::ById(_) => None,
            Self::Expr(pred) => Some(pred),
        }
    }

    pub(crate) fn matches(&self, doc: &Document) -> bool {
        match self {
            Self::All => true,
            Self::ById(id) => doc.get("_id") == Some(id),
            Self::Expr(pred) => pred.matches(doc),
        }
    }

    /// Collect matching documents, optionally capped at `limit` (None = all).
    pub(crate) async fn collect(
        &self,
        registry: &CollectionRegistry,
        db: &str,
        coll: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Document>> {
        if limit == Some(0) {
            return Ok(Vec::new());
        }

        match self {
            Self::ById(id) => match registry.get(db, coll, id).await? {
                Some(doc) => Ok(vec![doc]),
                None => Ok(Vec::new()),
            },
            Self::Expr(_) => {
                if let Some(id) = self.predicate().and_then(|p| p.extract_id_eq()) {
                    let Some(doc) = registry.get(db, coll, id).await? else {
                        return Ok(Vec::new());
                    };
                    if self.matches(&doc) {
                        Ok(vec![doc])
                    } else {
                        Ok(Vec::new())
                    }
                } else {
                    let scanned = registry.scan(db, coll, None).await?;
                    let mut matched = Vec::new();
                    for doc in scanned {
                        if self.matches(&doc) {
                            matched.push(doc);
                            if let Some(max) = limit {
                                if matched.len() >= max {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(matched)
                }
            }
            Self::All => {
                let scan_limit = limit.map(|n| n as i32);
                registry.scan(db, coll, scan_limit).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;

    #[test]
    fn parses_all_by_id_and_equality() {
        assert_eq!(QueryFilter::from_query(&doc! {}).unwrap(), QueryFilter::All);
        assert_eq!(
            QueryFilter::from_query(&doc! { "_id": "alice" }).unwrap(),
            QueryFilter::ById(Bson::String("alice".into()))
        );
        assert!(matches!(
            QueryFilter::from_query(&doc! { "name": "alice" }).unwrap(),
            QueryFilter::Expr(_)
        ));
    }

    #[test]
    fn parses_comparison_operators() {
        let filter = QueryFilter::from_query(&doc! { "score": { "$gt": 10 } }).unwrap();
        assert!(filter.matches(&doc! { "score": 11 }));
        assert!(!filter.matches(&doc! { "score": 10 }));
    }

    #[test]
    fn matches_and_equality() {
        let filter =
            QueryFilter::from_query(&doc! { "name": "alice", "score": 10 }).unwrap();
        assert!(filter.matches(&doc! { "name": "alice", "score": 10, "extra": true }));
        assert!(!filter.matches(&doc! { "name": "alice", "score": 11 }));
        assert!(!filter.matches(&doc! { "name": "bob", "score": 10 }));
    }
}
