# modmail

## 環境変数
.envを使って、保存することもできます。
- `DISCORD_TOKEN` - ボットのトークン
- `CATEGORY_ID` - カテゴリーのID
- `GUILD_ID` - サーバのID

## 実行
メモリーはあまり食いません。
```sh
cargo run --release
```

## 開発者向け

### Linux => Windows
以下を実行しなければいけません。
```sh
sudo apt install mingw-w64
```