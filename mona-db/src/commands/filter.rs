use bson::{Bson, Document};

use crate::error::{Error, Result};
use crate::storage::{document_matches_equality, CollectionRegistry};

/// Supported write/find query shapes: empty, sole `_id`, or top-level equality AND.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QueryFilter {
    All,
    ById(Bson),
    Equality(Document),
}

impl QueryFilter {
    pub(crate) fn from_query(query: &Document) -> Result<Self> {
        if query.is_empty() {
            return Ok(Self::All);
        }

        for key in query.keys() {
            if key.starts_with('$') {
                return Err(Error::CommandParse(format!(
                    "unsupported query operator: '{key}' (only top-level equality is supported)"
                )));
            }
        }

        if query.len() == 1 {
            if let Some(id) = query.get("_id") {
                return Ok(Self::ById(id.clone()));
            }
        }

        Ok(Self::Equality(query.clone()))
    }

    /// Equality predicate document for scan filtering (`None` means match all).
    pub(crate) fn equality_doc(&self) -> Option<&Document> {
        match self {
            Self::All | Self::ById(_) => None,
            Self::Equality(doc) => Some(doc),
        }
    }

    pub(crate) fn matches(&self, doc: &Document) -> bool {
        match self {
            Self::All => true,
            Self::ById(id) => doc.get("_id") == Some(id),
            Self::Equality(eq) => document_matches_equality(doc, eq),
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
            Self::Equality(eq) => {
                if let Some(id) = eq.get("_id") {
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
        assert_eq!(
            QueryFilter::from_query(&doc! { "name": "alice" }).unwrap(),
            QueryFilter::Equality(doc! { "name": "alice" })
        );
        assert_eq!(
            QueryFilter::from_query(&doc! { "name": "alice", "score": 10 }).unwrap(),
            QueryFilter::Equality(doc! { "name": "alice", "score": 10 })
        );
    }

    #[test]
    fn rejects_dollar_operators() {
        let err = QueryFilter::from_query(&doc! { "$gt": { "score": 5 } }).unwrap_err();
        assert!(err.to_string().contains("unsupported query operator"));
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
