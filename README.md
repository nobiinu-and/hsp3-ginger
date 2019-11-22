# HSP3 GINGER

HSP3 開発ツールを作るプロジェクト。

## プロジェクト

### hsp3-analyzer-mini

[hsp3-analyzer-mini](hsp3-analyzer-mini)

- LSP クライアント (入力補完など機能)
- VSCode に拡張機能をインストールすることで入力補完やホバーなどの支援を受けられます。
- 言語: Rust
- 状況: 基本的な機能は実装済み。最低限の実用可能なレベルです。マクロや複数行文字列など、複雑な機能には対応していません。

### hsp3-debug-empty

[hsp3-debug-empty](hsp3-debug-empty)

- 何もしないデバッガー
- 新しいデバッガーを作るときの土台
- 言語: C++

### hsp3-debug-ginger

[hsp3-debug-ginger](hsp3-debug-ginger)

- VSCode 用デバッガー
- Debug Adapter Protocol 対応
- 言語: Rust
- 状況: アルファ版リリース済み。まだ実用レベルではありません。

### hsp3-debug-self

[hsp3-debug-self](hsp3-debug-self)

- サーバーとクライアントに分離したデバッガー
- 言語: C++ (サーバー), HSP (クライアント)
- 状況: 概念実証レベル。コンセプトは [knowbug v2](https://github.com/vain0x) に引き継がれました。

### hsp3-debug-spider

[hsp3-debug-spider](hsp3-debug-spider)

- デバッガー
- Web サーバーとブラウザを起動することで、GUI を HTML/CSS により実装しています。
- 言語: Rust (サーバー),　JavaScript (クライアント), C# (ブラウザ)
- 状況: 概念実証 (proof-of-concept) 済み。実用レベルではありません。

### hsp3-ginger

[hsp3-ginger](hsp3-ginger)

- コマンドラインコンパイラを作ろうとしていたもの。
- 状況: 作業途中

### vscode-ext

[vscode-ext](vscode-ext)

- VSCode 拡張機能
- 言語: TypeScript
- 状況: シンタックスハイライトのみ
- 備考: [honobonosun/vscode-language-hsp3](https://github.com/honobonosun/vscode-language-hsp3) を使ってください。

## その他

以下は他のリポジトリのコードの再配布です。

### language-hsp3

[language-hsp3](https://github.com/honobonosun/language-hsp3)

- vscode-ext から参照されます。
