Output docs/tapes/quota.gif
Set FontSize 15
Set Width 900
Set Height 480
Set Theme "Dracula"
Set Padding 30

Type "./target/debug/lastdown quota elci/fixtures/quota_limit.json"
Enter
Sleep 1s

Type "./target/debug/lastdown bench elci/fixtures/quota_limit.json"
Enter
Sleep 2s
