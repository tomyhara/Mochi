# Mochi ドキュメント

**Mochi** は、Codex CLI / Claude Code / OpenCode のセッションを
**リポジトリ単位で集約・閲覧・検索・再開**する、Windows / macOS 向けデスクトップアプリケーションです。

## ドキュメント一覧

| ドキュメント | 内容 |
| --- | --- |
| [requirements.md](./requirements.md) | **要件定義書** — 背景・スコープ・機能要件（FR）・非機能要件（NFR）・データモデル・リスク・MVP 完了条件 |
| [tool-integration.md](./tool-integration.md) | **ツール連携仕様（付録）** — 3 ツールのセッション保存場所・形式・再開コマンドの調査結果と、実装前に確定すべき事項 |

## 現在のステータス

| 項目 | 状態 |
| --- | --- |
| 要件定義 | ドラフト v0.1 — **レビュー待ち** |
| 未決事項 | [requirements.md §11](./requirements.md#11-未決事項要確認) に 7 件（内蔵ターミナルの要否、削除機能の要否 など） |
| 次のステップ | M0（調査・PoC）— 実 CLI からセッションを採取し、[tool-integration.md §5](./tool-integration.md#5-m0調査pocフェーズで確定すべきこと) のチェックリストを消化する |
