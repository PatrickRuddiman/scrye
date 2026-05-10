use scryd_search::{
    InMemoryIndexer, Indexer, IndexSubmit, MessageId, Mode, SearchQuery,
};

fn submit(id: &str, doc: &str) -> IndexSubmit {
    IndexSubmit {
        message_id: MessageId::new(id),
        document: doc.to_string(),
    }
}

#[tokio::test]
async fn submit_then_search_returns_match() {
    let idx = InMemoryIndexer::new();
    idx.submit(submit("primary:a@x", "April invoice attached"))
        .await
        .unwrap();
    idx.submit(submit("primary:b@x", "lunch plans"))
        .await
        .unwrap();

    let resp = idx
        .search(&SearchQuery {
            q: "invoice".to_string(),
            mode: Mode::FullText,
            k: 10,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(resp.hits.len(), 1);
    assert_eq!(resp.hits[0].message_id.as_str(), "primary:a@x");
}

#[tokio::test]
async fn duplicate_submit_replaces_existing_document() {
    let idx = InMemoryIndexer::new();
    idx.submit(submit("primary:a@x", "version one"))
        .await
        .unwrap();
    idx.submit(submit("primary:a@x", "version two"))
        .await
        .unwrap();

    let resp = idx
        .search(&SearchQuery {
            q: "two".to_string(),
            mode: Mode::FullText,
            k: 10,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(resp.hits.len(), 1);
    assert_eq!(idx.len(), 1);
}

#[tokio::test]
async fn remove_deletes_document() {
    let idx = InMemoryIndexer::new();
    idx.submit(submit("primary:a@x", "remove me"))
        .await
        .unwrap();
    idx.remove(&MessageId::new("primary:a@x")).await.unwrap();
    let resp = idx
        .search(&SearchQuery {
            q: "remove".to_string(),
            mode: Mode::FullText,
            k: 10,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert!(resp.hits.is_empty());
}

#[tokio::test]
async fn truncate_clears_everything() {
    let idx = InMemoryIndexer::new();
    for i in 0..5 {
        idx.submit(submit(&format!("primary:m{i}@x"), "body content"))
            .await
            .unwrap();
    }
    idx.truncate().await.unwrap();
    assert!(idx.is_empty());
}

#[tokio::test]
async fn search_orders_by_term_count() {
    let idx = InMemoryIndexer::new();
    idx.submit(submit("primary:less@x", "invoice mentioned once"))
        .await
        .unwrap();
    idx.submit(submit(
        "primary:more@x",
        "invoice invoice and another invoice",
    ))
    .await
    .unwrap();

    let resp = idx
        .search(&SearchQuery {
            q: "invoice".to_string(),
            mode: Mode::FullText,
            k: 10,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(resp.hits.len(), 2);
    assert_eq!(resp.hits[0].message_id.as_str(), "primary:more@x");
    assert!(resp.hits[0].score > resp.hits[1].score);
}

#[tokio::test]
async fn search_respects_k_limit() {
    let idx = InMemoryIndexer::new();
    for i in 0..10 {
        idx.submit(submit(&format!("primary:m{i}@x"), "common term"))
            .await
            .unwrap();
    }
    let resp = idx
        .search(&SearchQuery {
            q: "common".to_string(),
            mode: Mode::FullText,
            k: 3,
            account_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(resp.hits.len(), 3);
}

#[test]
fn mode_round_trips_through_string() {
    use std::str::FromStr;
    for m in [Mode::FullText, Mode::Semantic, Mode::Hybrid] {
        let s = m.to_string();
        assert_eq!(Mode::from_str(&s).unwrap(), m);
    }
    assert!(Mode::from_str("garbage").is_err());
}
