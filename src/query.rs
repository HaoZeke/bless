//! Local (and optional MongoDB) query surface: `ls`, `show`, `fetch`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::cli::{Cli, QueryCommand};
use crate::error::BlessError;

const UUID_LEN: usize = 36;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalRunFile {
    pub uuid: String,
    pub label: String,
    pub stream: Option<String>,
    pub filename: String,
    pub size: u64,
    pub mtime: SystemTime,
}

pub(crate) async fn run(query: &QueryCommand, cli: &Cli) -> Result<(), BlessError> {
    #[cfg(feature = "mongodb")]
    if cli.use_mongodb {
        return run_mongo(query, cli).await;
    }
    #[cfg(not(feature = "mongodb"))]
    let _ = cli;
    let dir = std::env::current_dir()?;
    run_local(query, &dir)
}

fn run_local(query: &QueryCommand, dir: &Path) -> Result<(), BlessError> {
    match query {
        QueryCommand::Ls => {
            let runs = list_local(dir)?;
            print!("{}", format_local_ls(&runs));
            Ok(())
        }
        QueryCommand::Show { id } => {
            let runs = list_local(dir)?;
            let matched = resolve_id(&runs, id)?;
            print!("{}", format_local_show(&matched));
            Ok(())
        }
        QueryCommand::Fetch { id, output } => {
            let runs = list_local(dir)?;
            let matched = resolve_id(&runs, id)?;
            fetch_local(dir, &matched, output.as_deref())
        }
    }
}

#[cfg(feature = "mongodb")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct MongoRunRow {
    run_uuid: String,
    label: String,
    stream: String,
    storage: String,
    size_bytes: i64,
    start_time: String,
}

#[cfg(feature = "mongodb")]
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(feature = "mongodb")]
fn run_uuid_filter(id: &str) -> mongodb::bson::Document {
    use mongodb::bson::doc;
    if is_uuid_like(id) {
        doc! { "run_uuid": id }
    } else {
        doc! { "run_uuid": { "$regex": format!("^{}", regex_escape(id)), "$options": "i" } }
    }
}

#[cfg(feature = "mongodb")]
fn mongo_row_from_doc(doc: &mongodb::bson::Document) -> MongoRunRow {
    let run_uuid = doc.get_str("run_uuid").unwrap_or("-").to_string();
    let label = doc.get_str("label").unwrap_or("-").to_string();
    let stream = doc.get_str("stream").unwrap_or("").to_string();
    let storage = if let Ok(s) = doc.get_str("storage") {
        s.to_string()
    } else if doc.contains_key("gzip_blob_id") {
        "gridfs".into()
    } else if doc.contains_key("gzip_blob") {
        "bson".into()
    } else {
        "-".into()
    };
    let size_bytes = doc
        .get_i64("size_bytes")
        .unwrap_or_else(|_| doc.get_i32("size_bytes").map(i64::from).unwrap_or(0));
    let start_time = doc
        .get_datetime("start_time")
        .ok()
        .and_then(|dt| dt.try_to_rfc3339_string().ok())
        .unwrap_or_else(|| "-".into());
    MongoRunRow {
        run_uuid,
        label,
        stream,
        storage,
        size_bytes,
        start_time,
    }
}

#[cfg(feature = "mongodb")]
fn unique_mongo_docs(
    docs: Vec<mongodb::bson::Document>,
    id: &str,
) -> Result<Vec<mongodb::bson::Document>, BlessError> {
    if id.is_empty() {
        return Err(BlessError::Config(
            "id must be a run uuid or unique prefix".into(),
        ));
    }
    let mut uuids: Vec<String> = docs
        .iter()
        .filter_map(|d| d.get_str("run_uuid").ok().map(str::to_string))
        .filter(|u| uuid_matches_id(u, id))
        .collect();
    uuids.sort();
    uuids.dedup();
    match uuids.as_slice() {
        [] => Err(BlessError::Config(format!("no run matching '{id}'"))),
        [uuid] => {
            let mut docs: Vec<_> = docs
                .into_iter()
                .filter(|d| d.get_str("run_uuid").ok() == Some(uuid.as_str()))
                .collect();
            docs.sort_by(|a, b| {
                stream_sort_key(&Some(a.get_str("stream").unwrap_or("").to_string())).cmp(
                    &stream_sort_key(&Some(b.get_str("stream").unwrap_or("").to_string())),
                )
            });
            Ok(docs)
        }
        many => Err(BlessError::Config(format!(
            "ambiguous id '{id}' matches {} runs",
            many.len()
        ))),
    }
}

#[cfg(feature = "mongodb")]
async fn run_mongo(query: &QueryCommand, cli: &Cli) -> Result<(), BlessError> {
    use crate::db::setup_mongodb;
    use crate::storage_backends::mongodb::MongoDBStorage;
    use mongodb::bson::doc;

    let client = setup_mongodb().await?;
    let storage = MongoDBStorage::new(&client, &cli.db, &cli.collection).await?;

    match query {
        QueryCommand::Ls => {
            let docs = storage.find_docs(doc! {}, false).await?;
            let rows: Vec<_> = docs.iter().map(mongo_row_from_doc).collect();
            print!("{}", format_mongo_ls(&rows));
            Ok(())
        }
        QueryCommand::Show { id } => {
            let docs = storage.find_docs(run_uuid_filter(id), false).await?;
            let docs = unique_mongo_docs(docs, id)?;
            let rows: Vec<_> = docs.iter().map(mongo_row_from_doc).collect();
            print!("{}", format_mongo_ls(&rows));
            Ok(())
        }
        QueryCommand::Fetch { id, output } => {
            let docs = storage.find_docs(run_uuid_filter(id), true).await?;
            let docs = unique_mongo_docs(docs, id)?;
            fetch_mongo(&storage, &docs, output.as_deref()).await
        }
    }
}

pub(crate) fn is_uuid_like(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != UUID_LEN {
        return false;
    }
    let dashes = [8usize, 13, 18, 23];
    if dashes.iter().any(|&i| b[i] != b'-') {
        return false;
    }
    let is_hex = |c: u8| c.is_ascii_hexdigit();
    (0..8).all(|i| is_hex(b[i]))
        && (9..13).all(|i| is_hex(b[i]))
        && (14..18).all(|i| is_hex(b[i]))
        && (19..23).all(|i| is_hex(b[i]))
        && (24..36).all(|i| is_hex(b[i]))
}

pub(crate) fn uuid_matches_id(uuid: &str, id: &str) -> bool {
    let uuid = uuid.to_ascii_lowercase();
    let id = id.to_ascii_lowercase();
    uuid == id || uuid.starts_with(&id)
}

pub(crate) fn parse_run_filename(name: &str) -> Option<(String, String, Option<String>)> {
    let stem = name.strip_suffix(".log.gz")?;
    let (stem, stream) = if let Some(rest) = stem.strip_suffix("_stdout") {
        (rest, Some("stdout".to_string()))
    } else if let Some(rest) = stem.strip_suffix("_stderr") {
        (rest, Some("stderr".to_string()))
    } else {
        (stem, None)
    };
    // `{label}_{uuid}` — UUID is 36 chars, preceded by `_`.
    if stem.len() < UUID_LEN + 2 {
        return None;
    }
    let split_at = stem.len() - UUID_LEN - 1;
    let (label, uuid_part) = stem.split_at(split_at);
    if !uuid_part.starts_with('_') {
        return None;
    }
    let uuid = &uuid_part[1..];
    if label.is_empty() || !is_uuid_like(uuid) {
        return None;
    }
    Some((uuid.to_ascii_lowercase(), label.to_string(), stream))
}

pub(crate) fn list_local(dir: &Path) -> Result<Vec<LocalRunFile>, BlessError> {
    let mut runs = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        let Some((uuid, label, stream)) = parse_run_filename(&filename) else {
            continue;
        };
        let meta = fs::metadata(&path)?;
        runs.push(LocalRunFile {
            uuid,
            label,
            stream,
            filename,
            size: meta.len(),
            mtime: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    runs.sort_by(|a, b| {
        a.uuid
            .cmp(&b.uuid)
            .then_with(|| stream_sort_key(&a.stream).cmp(&stream_sort_key(&b.stream)))
            .then_with(|| a.filename.cmp(&b.filename))
    });
    Ok(runs)
}

fn stream_sort_key(stream: &Option<String>) -> u8 {
    match stream.as_deref() {
        None | Some("") => 0,
        Some("stdout") => 1,
        Some("stderr") => 2,
        _ => 3,
    }
}

fn stream_display(stream: &Option<String>) -> &str {
    match stream.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => "-",
    }
}

pub(crate) fn resolve_id<'a>(
    runs: &'a [LocalRunFile],
    id: &str,
) -> Result<Vec<&'a LocalRunFile>, BlessError> {
    if id.is_empty() {
        return Err(BlessError::Config(
            "id must be a run uuid or unique prefix".into(),
        ));
    }
    let matched: Vec<&LocalRunFile> = runs
        .iter()
        .filter(|r| uuid_matches_id(&r.uuid, id))
        .collect();
    let mut uuids: Vec<&str> = matched.iter().map(|r| r.uuid.as_str()).collect();
    uuids.sort_unstable();
    uuids.dedup();
    match uuids.as_slice() {
        [] => Err(BlessError::Config(format!("no run matching '{id}'"))),
        [_] => Ok(matched),
        many => Err(BlessError::Config(format!(
            "ambiguous id '{id}' matches {} runs",
            many.len()
        ))),
    }
}

fn format_mtime(mtime: SystemTime) -> String {
    match mtime.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(_) => humantime::format_rfc3339_seconds(mtime).to_string(),
        Err(_) => "-".into(),
    }
}

pub(crate) fn format_local_ls(runs: &[LocalRunFile]) -> String {
    let mut out = String::from("uuid\tlabel\tstream\tpath\tsize\n");
    for run in runs {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            run.uuid,
            run.label,
            stream_display(&run.stream),
            run.filename,
            run.size
        ));
    }
    out
}

pub(crate) fn format_local_show(runs: &[&LocalRunFile]) -> String {
    let mut out = String::from("uuid\tlabel\tstream\tpath\tsize\tmtime\n");
    for run in runs {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            run.uuid,
            run.label,
            stream_display(&run.stream),
            run.filename,
            run.size,
            format_mtime(run.mtime)
        ));
    }
    out
}

fn fetch_local(
    dir: &Path,
    matched: &[&LocalRunFile],
    output: Option<&str>,
) -> Result<(), BlessError> {
    if matched.is_empty() {
        return Err(BlessError::Config("no run matching id".into()));
    }
    let uuid = &matched[0].uuid;
    match output {
        Some("-") => {
            if matched.len() > 1 {
                return Err(split_single_dest_error(uuid));
            }
            let src = dir.join(&matched[0].filename);
            let mut stdout = io::stdout();
            let mut file = fs::File::open(src)?;
            io::copy(&mut file, &mut stdout)?;
            stdout.flush()?;
        }
        Some(path) => {
            if matched.len() > 1 {
                return Err(split_single_dest_error(uuid));
            }
            let dest = dest_path(dir, path);
            fs::copy(dir.join(&matched[0].filename), dest)?;
        }
        None if matched.len() > 1 => {
            for run in matched {
                let stream = run.stream.as_deref().ok_or_else(|| {
                    BlessError::Config(format!("split run {uuid} has a file without a stream name"))
                })?;
                let dest = dir.join(format!("{uuid}_{stream}.log.gz"));
                fs::copy(dir.join(&run.filename), dest)?;
            }
        }
        None => {
            let dest = dir.join(format!("{uuid}.log.gz"));
            fs::copy(dir.join(&matched[0].filename), dest)?;
        }
    }
    Ok(())
}

fn dest_path(dir: &Path, output: &str) -> PathBuf {
    let path = Path::new(output);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        dir.join(path)
    }
}

fn split_single_dest_error(uuid: &str) -> BlessError {
    BlessError::Config(format!(
        "run {uuid} has two streams; omit -o to write both or pass a prefix that matches one file"
    ))
}

#[cfg(feature = "mongodb")]
fn format_mongo_ls(rows: &[MongoRunRow]) -> String {
    let mut out = String::from("run_uuid\tlabel\tstream\tstorage\tsize_bytes\tstart_time\n");
    for row in rows {
        let stream = if row.stream.is_empty() {
            "-"
        } else {
            row.stream.as_str()
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            row.run_uuid, row.label, stream, row.storage, row.size_bytes, row.start_time
        ));
    }
    out
}

#[cfg(feature = "mongodb")]
async fn fetch_mongo(
    storage: &crate::storage_backends::mongodb::MongoDBStorage,
    docs: &[mongodb::bson::Document],
    output: Option<&str>,
) -> Result<(), BlessError> {
    if docs.is_empty() {
        return Err(BlessError::Config("no run matching id".into()));
    }
    let uuid = docs[0]
        .get_str("run_uuid")
        .map_err(|_| BlessError::Config("document missing run_uuid".into()))?
        .to_string();

    match output {
        Some("-") => {
            if docs.len() > 1 {
                return Err(split_single_dest_error(&uuid));
            }
            let mut buf = Vec::new();
            storage.write_gzip_blob(&docs[0], &mut buf).await?;
            let mut stdout = io::stdout();
            stdout.write_all(&buf)?;
            stdout.flush()?;
        }
        Some(path) => {
            if docs.len() > 1 {
                return Err(split_single_dest_error(&uuid));
            }
            let dest = dest_path(&std::env::current_dir()?, path);
            let file = tokio::fs::File::create(&dest).await?;
            storage.write_gzip_blob(&docs[0], file).await?;
        }
        None if docs.len() > 1 => {
            for doc in docs {
                let stream = doc.get_str("stream").unwrap_or("");
                if stream.is_empty() {
                    return Err(BlessError::Config(format!(
                        "split run {uuid} has a document without a stream name"
                    )));
                }
                let dest = format!("{uuid}_{stream}.log.gz");
                let file = tokio::fs::File::create(&dest).await?;
                storage.write_gzip_blob(doc, file).await?;
            }
        }
        None => {
            let dest = format!("{uuid}.log.gz");
            let file = tokio::fs::File::create(&dest).await?;
            storage.write_gzip_blob(&docs[0], file).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    const UUID_A: &str = "11111111-1111-1111-1111-111111111111";
    const UUID_B: &str = "22222222-2222-2222-2222-222222222222";
    const UUID_C: &str = "11112222-2222-2222-2222-222222222222";

    fn write_gz(dir: &Path, name: &str, bytes: &[u8]) {
        fs::write(dir.join(name), bytes).unwrap();
    }

    #[test]
    fn uuid_like_accepts_hyphenated_hex() {
        assert!(is_uuid_like(UUID_A));
        assert!(is_uuid_like("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"));
        assert!(!is_uuid_like("11111111-1111-1111-1111-11111111111"));
        assert!(!is_uuid_like("not-a-uuid"));
        assert!(!is_uuid_like(""));
    }

    #[test]
    fn parse_combined_and_split_names() {
        assert_eq!(
            parse_run_filename(&format!("myrun_{UUID_A}.log.gz")),
            Some((UUID_A.into(), "myrun".into(), None))
        );
        assert_eq!(
            parse_run_filename(&format!("my_lab_{UUID_A}_stdout.log.gz")),
            Some((UUID_A.into(), "my_lab".into(), Some("stdout".into())))
        );
        assert_eq!(
            parse_run_filename(&format!("my_lab_{UUID_A}_stderr.log.gz")),
            Some((UUID_A.into(), "my_lab".into(), Some("stderr".into())))
        );
        assert_eq!(parse_run_filename("build_log.gz"), None);
        assert_eq!(parse_run_filename("random.log.gz"), None);
        assert_eq!(parse_run_filename(&format!("{UUID_A}.log.gz")), None);
        assert_eq!(parse_run_filename("notes.txt"), None);
    }

    #[test]
    fn list_local_skips_non_run_names() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("myrun_{UUID_A}.log.gz"), b"aaa");
        write_gz(dir.path(), &format!("job_{UUID_B}_stdout.log.gz"), b"bbbb");
        write_gz(dir.path(), &format!("job_{UUID_B}_stderr.log.gz"), b"cc");
        write_gz(dir.path(), "build_log.gz", b"nope");
        write_gz(dir.path(), "readme.txt", b"nope");

        let runs = list_local(dir.path()).unwrap();
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].uuid, UUID_A);
        assert_eq!(runs[0].label, "myrun");
        assert_eq!(runs[0].stream, None);
        assert_eq!(runs[0].size, 3);
        assert_eq!(runs[1].uuid, UUID_B);
        assert_eq!(runs[1].stream.as_deref(), Some("stdout"));
        assert_eq!(runs[2].uuid, UUID_B);
        assert_eq!(runs[2].stream.as_deref(), Some("stderr"));
    }

    #[test]
    fn ls_format_has_expected_columns() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("lab_{UUID_A}.log.gz"), b"xx");
        let runs = list_local(dir.path()).unwrap();
        let text = format_local_ls(&runs);
        assert!(text.starts_with("uuid\tlabel\tstream\tpath\tsize\n"));
        let row = text.lines().nth(1).unwrap();
        let cols: Vec<_> = row.split('\t').collect();
        assert_eq!(
            cols,
            vec![UUID_A, "lab", "-", &format!("lab_{UUID_A}.log.gz"), "2"]
        );
    }

    #[test]
    fn show_adds_mtime_and_resolves_prefix() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("lab_{UUID_A}.log.gz"), b"xx");
        write_gz(dir.path(), &format!("other_{UUID_B}.log.gz"), b"yy");
        let runs = list_local(dir.path()).unwrap();
        let matched = resolve_id(&runs, "11111111").unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].uuid, UUID_A);
        let text = format_local_show(&matched);
        assert!(text.starts_with("uuid\tlabel\tstream\tpath\tsize\tmtime\n"));
        let cols: Vec<_> = text.lines().nth(1).unwrap().split('\t').collect();
        assert_eq!(cols[0], UUID_A);
        assert_eq!(cols.len(), 6);
        assert!(cols[5].contains('T'), "mtime {}", cols[5]);
    }

    #[test]
    fn resolve_id_ambiguous_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("a_{UUID_A}.log.gz"), b"x");
        write_gz(dir.path(), &format!("c_{UUID_C}.log.gz"), b"y");
        let runs = list_local(dir.path()).unwrap();
        let err = resolve_id(&runs, "1111").unwrap_err();
        match err {
            BlessError::Config(msg) => {
                assert!(msg.contains("ambiguous"), "{msg}");
            }
            other => panic!("{other:?}"),
        }
        let err = resolve_id(&runs, "zzzz").unwrap_err();
        match err {
            BlessError::Config(msg) => {
                assert!(msg.contains("no run matching"), "{msg}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fetch_combined_default_name() {
        let dir = tempfile::tempdir().unwrap();
        let src_name = format!("lab_{UUID_A}.log.gz");
        write_gz(dir.path(), &src_name, b"payload");
        let runs = list_local(dir.path()).unwrap();
        let matched = resolve_id(&runs, UUID_A).unwrap();
        fetch_local(dir.path(), &matched, None).unwrap();
        let dest = dir.path().join(format!("{UUID_A}.log.gz"));
        assert_eq!(fs::read(dest).unwrap(), b"payload");
    }

    #[test]
    fn fetch_split_writes_both_unless_dash_o() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("job_{UUID_B}_stdout.log.gz"), b"out");
        write_gz(dir.path(), &format!("job_{UUID_B}_stderr.log.gz"), b"err");
        let runs = list_local(dir.path()).unwrap();
        let matched = resolve_id(&runs, "2222").unwrap();
        assert_eq!(matched.len(), 2);
        fetch_local(dir.path(), &matched, None).unwrap();
        assert_eq!(
            fs::read(dir.path().join(format!("{UUID_B}_stdout.log.gz"))).unwrap(),
            b"out"
        );
        assert_eq!(
            fs::read(dir.path().join(format!("{UUID_B}_stderr.log.gz"))).unwrap(),
            b"err"
        );

        let err = fetch_local(dir.path(), &matched, Some("one.log.gz")).unwrap_err();
        match err {
            BlessError::Config(msg) => {
                assert!(msg.contains("two streams"), "{msg}");
            }
            other => panic!("{other:?}"),
        }
        let err = fetch_local(dir.path(), &matched, Some("-")).unwrap_err();
        match err {
            BlessError::Config(msg) => {
                assert!(msg.contains("two streams"), "{msg}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fetch_dash_o_copies_one_file() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("lab_{UUID_A}.log.gz"), b"only");
        let runs = list_local(dir.path()).unwrap();
        let matched = resolve_id(&runs, "11111111-1111").unwrap();
        fetch_local(dir.path(), &matched, Some("copy.log.gz")).unwrap();
        assert_eq!(fs::read(dir.path().join("copy.log.gz")).unwrap(), b"only");
    }

    #[test]
    fn show_split_includes_both_files() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("job_{UUID_B}_stdout.log.gz"), b"out");
        write_gz(dir.path(), &format!("job_{UUID_B}_stderr.log.gz"), b"err");
        let runs = list_local(dir.path()).unwrap();
        let matched = resolve_id(&runs, UUID_B).unwrap();
        let text = format_local_show(&matched);
        assert!(text.contains("stdout"));
        assert!(text.contains("stderr"));
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn list_mtime_is_file_mtime() {
        let dir = tempfile::tempdir().unwrap();
        write_gz(dir.path(), &format!("lab_{UUID_A}.log.gz"), b"xx");
        let before = SystemTime::now() - Duration::from_secs(5);
        let runs = list_local(dir.path()).unwrap();
        assert!(runs[0].mtime >= before);
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn mongo_row_from_document_fields() {
        use mongodb::bson::{doc, DateTime};
        let start = DateTime::from_millis(0);
        let doc = doc! {
            "run_uuid": UUID_A,
            "label": "nightly",
            "stream": "",
            "storage": "bson",
            "size_bytes": 12i64,
            "start_time": start,
        };
        let row = mongo_row_from_doc(&doc);
        assert_eq!(row.run_uuid, UUID_A);
        assert_eq!(row.label, "nightly");
        assert_eq!(row.stream, "");
        assert_eq!(row.storage, "bson");
        assert_eq!(row.size_bytes, 12);
        assert!(row.start_time.starts_with("1970-01-01"));
        let text = format_mongo_ls(&[row]);
        assert!(text.starts_with("run_uuid\tlabel\tstream\tstorage\tsize_bytes\tstart_time\n"));
        assert!(text.contains(UUID_A));
        assert!(text.contains("\t-\t"));
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn mongo_prefix_filter_and_unique_docs() {
        use mongodb::bson::doc;
        let a = doc! { "run_uuid": UUID_A, "stream": "stdout" };
        let a2 = doc! { "run_uuid": UUID_A, "stream": "stderr" };
        let c = doc! { "run_uuid": UUID_C, "stream": "" };
        let docs = unique_mongo_docs(vec![a.clone(), a2.clone()], "11111111").unwrap();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].get_str("stream").unwrap(), "stdout");
        assert_eq!(docs[1].get_str("stream").unwrap(), "stderr");
        let err = unique_mongo_docs(vec![a, c], "1111").unwrap_err();
        match err {
            BlessError::Config(msg) => assert!(msg.contains("ambiguous"), "{msg}"),
            other => panic!("{other:?}"),
        }
        let filter = run_uuid_filter("1111");
        assert!(filter.get_document("run_uuid").is_ok() || filter.get_str("run_uuid").is_ok());
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn regex_escape_dots() {
        assert_eq!(regex_escape("a.b"), "a\\.b");
        assert_eq!(regex_escape(UUID_A), UUID_A);
    }
}
