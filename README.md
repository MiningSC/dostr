# dostr


**D**iscord to n**ostr**.
Bot that forwards Discord messages to [Nostr](https://github.com/nostr-protocol/nostr).

Discord is used as an intermediary due to Twitter locking down it's API.

Reply to its message with `!help` and it will show you all available commands.

Powered by [nostr-bot](https://github.com/slaninas/nostr-bot.git) and Discord's API.

## How to run using Docker
```
git clone https://github.com/MiningSC/dostr/ && cd dostr
# Now add secret (hex private key) to config file, tune config if you wish to
./build_and_run.sh --clearnet|tor
```
Now the bot should be running and waiting for mentions. Just reply to its message to interact, see [Commands](#Commands).
It relays only new tweets that were posted after you launched it.

## Tor
In case `--tor` is used connections to both relay and Twitter *should* be going through tor. But if you need full anonymity please **check yourself there are no leaks**.

## How to cross-post Tweets
If you would like to cross-post tweets here is the process to follow:
1. Decide if you would like to create a new Discord server to store the tweet data or if you would like to use an existing Discord server which you are an administrator of.
2. Use a service such as TweetShift to post new tweets to a Discord channel on your server. Tweets from each twitter account should be posted in a different Discord channel.
3. In the Discord Developer Portal, create a new bot and give it access to your Discord server.  On the "URL Generator" page, the scope should be "bot". General permissions should be "Read Messages / View Channels". Text permissions should be "Read Message History".  Copy the "Generated URL" and paste it in a new browser tab.  Add the bot to the associated Discord server.
4. On the "Bot" page of the Discord Developer Portal, select the slider called "MESSAGE CONTENT INTENT".  
4. On the "General" page, click "Reset Secret" and save your Discord Bot API key.
5. Create and save a new Nostr private key for your main bot (you can use snort.social or any other Nostr key generating service).
6. Add the Nostr private key and the Discord API key to the config file.
7. Run the program.  Use the !add command from a Nostr Client to have the bot follow the discord channels you created in the following format: "!add channel-id:channel-name".  To get the channel-id you must have Developer Mode turned on for your Discord client.  Once this is turned on right click on the channel and click "Copy Channel ID".
8. The private keys for the bots are stored in the data/channels file.  If you would like to add pictures for your bots you can log into Nostr using these private keys and edit the bot profiles.
