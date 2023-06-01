# dostr


**D**iscord to n**ostr**.
Bot that forwards Discord messages to [Nostr](https://github.com/nostr-protocol/nostr) and includes automatic NIP05 verification and a webserver.

Discord is used as an intermediary due to Twitter locking down it's API.

Reply to its message with `!help` and it will show you all available commands.

Powered by [nostr-bot](https://github.com/slaninas/nostr-bot.git) and [serenity](https://github.com/serenity-rs/serenity).

## How to run using Docker
```
git clone https://github.com/MiningSC/dostr/ && cd dostr
# Now add secret (hex private key), Discord API key to .env file. Tune .env file if you wish to.  Make sure you remove the .example extention from the file.  
./build_and_run.sh --clearnet|tor
```
There is also a Docker Compose file if you prefer to use that.

Now the bot should be running and waiting for mentions. Just reply to its message to interact, see [Commands](#Commands).
It relays only new messages that were posted after you launched it.

## Update (6/1/2023)
Automatic NIP05 verification has been added as well as a folder for a static website. (webstatic)  I recommend using a reverse proxy such as Nginx Proxy Manager if you will use the NIP05 or website functions.  There is a variable for your domain in the .env file.  For every new account the bot follows, their public key and username are added to the nostr.json file for automatic verifications.

I have begun integrating Nitter into the project.  The bots now automatically pull their Display Name and Profile Picture from a Nitter RSS feed.  It is important that you include the correct twitter handle (all lowercase, no spaces), when you tell the bot to follow a new account so that it can properly populate these items.  There is a variable in the .env file for your preferred Nitter instance.  When the proper format to tell your bot to follow a discord channel is: "!add 1111088216607567974:twitterusername", where the numbers is the discord channel ID, followed by a colon with the twitter username after.  The twitter username should be a single word and not include the @.

There are now 3 mounts or volumes you must attach to the docker instance.  
  -One is for the file containing the followed channels, private keys and usernames (data/channels)
  -The second is for the nostr.json NIP 5 verificaiton (web)
  -The third is for the web server (webstatic)

## Tor
In case `--tor` is used connections to both relay and Twitter *should* be going through tor. But if you need full anonymity please **check yourself there are no leaks**.

## To Do
-Photos embeded in posts.  
-Web interface for adding new accounts to mirror.

## How to cross-post Tweets
If you would like to cross-post tweets here is the process to follow:
1. Decide if you would like to create a new Discord server to store the tweet data or if you would like to use an existing Discord server which you are an administrator of.
2. Use a service such as TweetShift to post new tweets to a Discord channel on your server. Tweets from each twitter account should be posted in a different Discord channel.
3. In the Discord Developer Portal, create a new bot and give it access to your Discord server.  On the "URL Generator" page, the scope should be "bot". General permissions should be "Read Messages / View Channels". Text permissions should be "Read Message History".  Copy the "Generated URL" and paste it in a new browser tab.  Add the bot to the associated Discord server.
4. On the "Bot" page of the Discord Developer Portal, select the slider called "MESSAGE CONTENT INTENT".  
4. On the "General" page, click "Reset Secret" and save your Discord Bot API key.
5. Create and save a new Nostr private key for your main bot (you can use snort.social or any other Nostr key generating service).
6. Add the Nostr private key and the Discord API key to the .env file.  Populate the other .env variables with your informatoin.
7. Run the program.  Use the !add command from a Nostr Client to have the bot follow the discord channels you created in the following format: "!add channel-id:twitterusername".  To get the channel-id you must have Developer Mode turned on for your Discord client.  Once this is turned on right click on the channel and click "Copy Channel ID".

## Known Issues
~~-When restarting the bot, if you previously populated photos or other bot profile information as referenced in step 8 above, those manual edits had to be re-entered.  To avoid this, the associated code in the dostr.rs file has been commented out.  You now will have to populate new account names, about sections, pictures or NIP-5 manually using the private key. When the bot is restarted it will not be erased.~~ - This has been resolved, now the profile picture, display name and NIP05 verifications are all done automatically.


