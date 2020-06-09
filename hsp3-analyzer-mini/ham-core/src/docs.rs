use crate::{rc_str::RcStr, syntax::DocId};
use encoding::{
    codec::utf_8::UTF8Encoding, label::encoding_from_windows_code_page, DecoderTrap, Encoding,
    StringWriter,
};
use lsp_types::*;
use notify::{DebouncedEvent, RecommendedWatcher};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

/// テキストドキュメントのバージョン番号
/// (エディタ上で編集されるたびに変わる番号。
///  いつの状態のテキストドキュメントを指しているかを明確にするためのもの。)
type TextDocumentVersion = i64;

const NO_VERSION: i64 = 1;

pub(crate) enum DocChange {
    Opened { doc: DocId, text: RcStr },
    Changed { doc: DocId, text: RcStr },
    Closed { doc: DocId },
}

/// テキストドキュメントを管理するもの。
#[derive(Default)]
pub(super) struct Docs {
    last_doc: usize,
    doc_to_uri: HashMap<DocId, Url>,
    uri_to_doc: HashMap<Url, DocId>,
    open_docs: HashSet<DocId>,
    doc_versions: HashMap<DocId, TextDocumentVersion>,
    // hsphelp や common の下をウォッチするのに使う
    #[allow(unused)]
    hsp_root: PathBuf,
    file_watcher: Option<RecommendedWatcher>,
    file_event_rx: Option<Receiver<DebouncedEvent>>,
    doc_changes: Vec<DocChange>,
}

impl Docs {
    pub(super) fn new(hsp_root: PathBuf) -> Self {
        Self {
            hsp_root,
            ..Default::default()
        }
    }

    // pub(crate) fn is_open(&self, uri: &Url) -> bool {
    //     self.uri_to_doc
    //         .get(&uri)
    //         .map_or(false, |doc| self.open_docs.contains(&doc))
    // }

    pub(crate) fn fresh_doc(&mut self) -> DocId {
        self.last_doc += 1;
        DocId::new(self.last_doc)
    }

    fn resolve_uri(&mut self, uri: Url) -> DocId {
        match self.uri_to_doc.get(&uri) {
            Some(&doc) => doc,
            None => {
                let doc = self.fresh_doc();
                self.doc_to_uri.insert(doc, uri.clone());
                self.uri_to_doc.insert(uri, doc);
                doc
            }
        }
    }

    pub(crate) fn find_by_uri(&self, uri: &Url) -> Option<DocId> {
        self.uri_to_doc.get(uri).cloned()
    }

    pub(crate) fn get_uri(&self, doc: DocId) -> Option<&Url> {
        self.doc_to_uri.get(&doc)
    }

    pub(crate) fn get_version(&self, doc: DocId) -> Option<TextDocumentVersion> {
        self.doc_versions.get(&doc).copied()
    }

    pub(crate) fn drain_doc_changes(&mut self, changes: &mut Vec<DocChange>) {
        changes.extend(self.doc_changes.drain(..));
    }

    pub(super) fn did_initialize(&mut self) {
        self.scan_files();

        if let Some((file_watcher, file_event_rx)) = self.start_file_watcher() {
            self.file_watcher = Some(file_watcher);
            self.file_event_rx = Some(file_event_rx);
        }
    }

    fn scan_files(&mut self) -> Option<()> {
        let current_dir = std::env::current_dir()
            .map_err(|err| warn!("カレントディレクトリの取得 {:?}", err))
            .ok()?;

        let glob_pattern = format!("{}/**/*.hsp", current_dir.to_str()?);

        debug!("ファイルリストを取得します '{}'", glob_pattern);

        let entries = match glob::glob(&glob_pattern) {
            Err(err) => {
                warn!("ファイルリストの取得 {:?}", err);
                return None;
            }
            Ok(entries) => entries,
        };

        for entry in entries {
            match entry {
                Err(err) => warn!("ファイルエントリの取得 {:?}", err),
                Ok(path) => {
                    self.change_file(&path);
                }
            }
        }

        None
    }

    fn start_file_watcher(&mut self) -> Option<(RecommendedWatcher, Receiver<DebouncedEvent>)> {
        debug!("ファイルウォッチャーを起動します");

        use notify::{RecursiveMode, Watcher};
        use std::sync::mpsc::channel;
        use std::time::Duration;

        let delay_millis = 1000;

        let current_dir = std::env::current_dir()
            .map_err(|err| warn!("カレントディレクトリの取得 {:?}", err))
            .ok()?;

        let (tx, rx) = channel();

        let mut watcher: RecommendedWatcher = Watcher::new(tx, Duration::from_millis(delay_millis))
            .map_err(|err| warn!("ファイルウォッチャーの作成 {:?}", err))
            .ok()?;

        watcher
            .watch(&current_dir, RecursiveMode::Recursive)
            .map_err(|err| warn!("ファイルウォッチャーの起動 {:?}", err))
            .ok()?;

        debug!("ファイルウォッチャーを起動しました ({:?})", current_dir);
        Some((watcher, rx))
    }

    pub(crate) fn poll(&mut self) {
        let rx = match self.file_event_rx.as_mut() {
            None => return,
            Some(rx) => rx,
        };

        debug!("ファイルウォッチャーのイベントをポールします。");

        let mut rescan = false;
        let mut updated_paths = HashSet::new();
        let mut removed_paths = HashSet::new();
        let mut disconnected = false;

        loop {
            match rx.try_recv() {
                Ok(DebouncedEvent::Create(ref path)) if file_ext_is_watched(path) => {
                    debug!("ファイルが作成されました: {:?}", path);
                    updated_paths.insert(path.clone());
                }
                Ok(DebouncedEvent::Write(ref path)) if file_ext_is_watched(path) => {
                    debug!("ファイルが変更されました: {:?}", path);
                    updated_paths.insert(path.clone());
                }
                Ok(DebouncedEvent::Remove(ref path)) if file_ext_is_watched(path) => {
                    debug!("ファイルが削除されました: {:?}", path);
                    removed_paths.insert(path.clone());
                }
                Ok(DebouncedEvent::Rename(ref src_path, ref dest_path)) => {
                    debug!("ファイルが移動しました: {:?} → {:?}", src_path, dest_path);
                    if file_ext_is_watched(src_path) {
                        removed_paths.insert(src_path.clone());
                    }
                    if file_ext_is_watched(dest_path) {
                        updated_paths.insert(dest_path.clone());
                    }
                }
                Ok(DebouncedEvent::Rescan) => {
                    debug!("ファイルウォッチャーから再スキャンが要求されました");
                    rescan = true;
                }
                Ok(ev) => {
                    debug!("ファイルウォッチャーのイベントをスキップします: {:?}", ev);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if rescan {
            self.scan_files();
        } else {
            for path in updated_paths {
                if removed_paths.contains(&path) {
                    continue;
                }
                self.change_file(&path);
            }

            for path in removed_paths {
                self.close_file(&path);
            }
        }

        if disconnected {
            self.shutdown_file_watcher();
        }
    }

    fn shutdown_file_watcher(&mut self) {
        debug!("ファイルウォッチャーがシャットダウンしました。");
        self.file_watcher.take();
        self.file_event_rx.take();
    }

    pub(super) fn shutdown(&mut self) {
        self.shutdown_file_watcher();
    }

    fn do_open_doc(&mut self, uri: Url, version: i64, text: RcStr) -> DocId {
        let doc = self.resolve_uri(uri);

        self.doc_versions.insert(doc, version);
        self.doc_changes.push(DocChange::Opened { doc, text });

        doc
    }

    fn do_change_doc(&mut self, uri: Url, version: i64, text: RcStr) {
        let doc = self.resolve_uri(uri);
        self.doc_versions.insert(doc, version);
        self.doc_changes.push(DocChange::Changed { doc, text });
    }

    fn do_close_doc(&mut self, uri: Url) {
        if let Some(&doc) = self.uri_to_doc.get(&uri) {
            self.doc_to_uri.remove(&doc);
            self.doc_changes.push(DocChange::Closed { doc })
        }

        self.uri_to_doc.remove(&uri);
    }

    pub(super) fn open_doc(&mut self, uri: Url, version: i64, text: String) {
        let uri = canonicalize_uri(uri);

        self.do_open_doc(uri.clone(), version, text.into());

        if let Some(&doc) = self.uri_to_doc.get(&uri) {
            self.open_docs.insert(doc);
        }

        self.poll();
    }

    pub(super) fn change_doc(&mut self, uri: Url, version: i64, text: String) {
        let uri = canonicalize_uri(uri);

        self.do_change_doc(uri.clone(), version, text.into());

        self.poll();
    }

    pub(super) fn close_doc(&mut self, uri: Url) {
        let uri = canonicalize_uri(uri);

        if let Some(&doc) = self.uri_to_doc.get(&uri) {
            self.open_docs.remove(&doc);
        }

        self.poll();
    }

    pub(super) fn change_file(&mut self, path: &Path) -> Option<()> {
        let shift_jis = encoding_from_windows_code_page(932).or_else(|| {
            warn!("shift_jis エンコーディングの取得");
            None
        })?;

        let uri = Url::from_file_path(path)
            .map_err(|err| warn!("URL の作成 {:?} {:?}", path, err))
            .ok()?;
        let uri = canonicalize_uri(uri);

        let is_open = self
            .uri_to_doc
            .get(&uri)
            .map_or(false, |doc| self.open_docs.contains(&doc));
        if is_open {
            debug!("ファイルは開かれているのでロードされません。");
            return None;
        }

        let mut text = String::new();
        if !read_file(path, &mut text, shift_jis) {
            warn!("ファイルを開けません {:?}", path);
        }

        self.do_change_doc(uri, NO_VERSION, text.into());

        None
    }

    pub(super) fn close_file(&mut self, path: &Path) -> Option<()> {
        let uri = Url::from_file_path(path)
            .map_err(|err| warn!("URL の作成 {:?} {:?}", path, err))
            .ok()?;

        let uri = canonicalize_uri(uri);

        self.do_close_doc(uri);

        None
    }
}

fn canonicalize_uri(uri: Url) -> Url {
    uri.to_file_path()
        .ok()
        .and_then(|path| path.canonicalize().ok())
        .and_then(|path| Url::from_file_path(path).ok())
        .unwrap_or(uri)
}

fn file_ext_is_watched(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == "hsp" || ext == "as")
}

/// ファイルを shift_jis または UTF-8 として読む。
fn read_file(file_path: &Path, out: &mut impl StringWriter, shift_jis: &dyn Encoding) -> bool {
    let content = match fs::read(file_path).ok() {
        None => return false,
        Some(x) => x,
    };

    shift_jis
        .decode_to(&content, DecoderTrap::Strict, out)
        .or_else(|_| UTF8Encoding.decode_to(&content, DecoderTrap::Strict, out))
        .is_ok()
}
