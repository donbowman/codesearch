//! `resolve_query` semantics for the TypeScript adapter (hand-populated
//! LMDB — no `scip-typescript` helper). Mirrors the C# fixture: two
//! `add` "overloads" sharing a simple name, one unique `mul`.

use super::*;

/// Writes scip_symbols / scip_simple_names / scip_positions directly and
/// returns the three canonical keys it stored.
fn populate_ambiguity_fixture(db_path: &Path) -> (String, String, String) {
    let env = crate::symbols::get_shared_scip_env(db_path).expect("shared env");
    let mut wtxn = env.write_txn().expect("wtxn");

    let add1 = "scip-typescript npm mypkg 1.0.0 `src/math.ts`/add().".to_string();
    let add2 = "scip-typescript npm mypkg 1.0.0 `src/math.ts`/add(`x`: number).".to_string();
    let mul = "scip-typescript npm mypkg 1.0.0 `src/math.ts`/mul().".to_string();

    let symbols: Database<Str, Bytes> = env
        .open_database(&wtxn, Some(SCIP_DB_NAME))
        .unwrap()
        .unwrap();
    for key in [&add1, &add2, &mul] {
        let refs = serialize_refs(&[StoredReference {
            file: PathBuf::from("src/math.ts"),
            start_line: 1,
            end_line: 1,
            kind: "definition".into(),
        }])
        .unwrap();
        symbols.put(&mut wtxn, key.as_str(), &refs).unwrap();
    }

    let names: Database<Str, Bytes> = env
        .open_database(&wtxn, Some(SCIP_NAMES_DB_NAME))
        .unwrap()
        .unwrap();
    // Stored deliberately out of order: resolution must sort.
    names
        .put(
            &mut wtxn,
            "add",
            &serialize_keys_v1(&[add2.clone(), add1.clone()]).unwrap(),
        )
        .unwrap();
    names
        .put(
            &mut wtxn,
            "mul",
            &serialize_keys_v1(std::slice::from_ref(&mul)).unwrap(),
        )
        .unwrap();

    let positions: Database<Str, Bytes> = env
        .open_database(&wtxn, Some(SCIP_POS_DB_NAME))
        .unwrap()
        .unwrap();
    positions
        .put(
            &mut wtxn,
            "src/math.ts:10",
            &serialize_keys_v1(&[add1.clone(), add2.clone()]).unwrap(),
        )
        .unwrap();
    positions
        .put(
            &mut wtxn,
            "src/math.ts:20",
            &serialize_keys_v1(std::slice::from_ref(&mul)).unwrap(),
        )
        .unwrap();

    wtxn.commit().unwrap();
    (add1, add2, mul)
}

#[test]
fn resolve_name_unique_fuzzy_resolves_the_single_candidate() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = dir.path().join("db");
    let (_a1, _a2, mul) = populate_ambiguity_fixture(&db);
    let indexer = TypeScriptSymbolIndexer::new();
    assert_eq!(
        indexer
            .resolve_query(&db, &ImpactQuery::Name("mul".into()))
            .unwrap(),
        KeyMatch::Resolved(mul)
    );
}

#[test]
fn resolve_name_overloads_come_back_ambiguous_and_sorted() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = dir.path().join("db");
    let (a1, a2, _mul) = populate_ambiguity_fixture(&db);
    let indexer = TypeScriptSymbolIndexer::new();
    // The pre-fix behaviour silently picked the shortest key here and
    // answered about the wrong overload.
    assert_eq!(
        indexer
            .resolve_query(&db, &ImpactQuery::Name("add".into()))
            .unwrap(),
        KeyMatch::Ambiguous(vec![a1, a2])
    );
}

#[test]
fn resolve_exact_key_is_verbatim_and_never_fuzzy() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = dir.path().join("db");
    let (a1, _a2, _mul) = populate_ambiguity_fixture(&db);
    let indexer = TypeScriptSymbolIndexer::new();
    assert_eq!(
        indexer
            .resolve_query(&db, &ImpactQuery::ExactKey(a1.clone()))
            .unwrap(),
        KeyMatch::Resolved(a1)
    );
    // A key that is only a prefix of a stored one must miss.
    assert_eq!(
        indexer
            .resolve_query(
                &db,
                &ImpactQuery::ExactKey("scip-typescript npm mypkg 1.0.0 `src/math.ts`/add".into())
            )
            .unwrap(),
        KeyMatch::NotFound
    );
}

#[test]
fn resolve_position_single_resolves_two_symbols_are_ambiguous() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = dir.path().join("db");
    let (a1, a2, mul) = populate_ambiguity_fixture(&db);
    let indexer = TypeScriptSymbolIndexer::new();

    assert_eq!(
        indexer
            .resolve_query(
                &db,
                &ImpactQuery::Position {
                    file: PathBuf::from("src/math.ts"),
                    line: 20
                }
            )
            .unwrap(),
        KeyMatch::Resolved(mul)
    );
    assert_eq!(
        indexer
            .resolve_query(
                &db,
                &ImpactQuery::Position {
                    file: PathBuf::from("src/math.ts"),
                    line: 10
                }
            )
            .unwrap(),
        KeyMatch::Ambiguous(vec![a1, a2])
    );
    assert_eq!(
        indexer
            .resolve_query(
                &db,
                &ImpactQuery::Position {
                    file: PathBuf::from("src/math.ts"),
                    line: 99
                }
            )
            .unwrap(),
        KeyMatch::NotFound
    );
}

#[test]
fn references_for_key_returns_stored_definitions() {
    let dir = tempfile::TempDir::new().unwrap();
    let db = dir.path().join("db");
    let (a1, _a2, _mul) = populate_ambiguity_fixture(&db);
    let indexer = TypeScriptSymbolIndexer::new();
    let refs = indexer.find_references_for_key(&db, &a1).unwrap();
    assert_eq!(refs.len(), 1, "definitions only, got {refs:?}");
    assert_eq!(refs[0].kind, "definition");
    assert_eq!(refs[0].file, PathBuf::from("src/math.ts"));
}
