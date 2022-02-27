### Usage
Two Matrix rooms are required to use this bot.

#### "Reporting" room
This room is open to everyone. Here people can share news any time. Editors can mark messages, but also images and videos with emoji reactions here. Use
- ⭕: Approve a message (to include it in the rendered markdown), or add an image/video to a previously reported item. The image/video will then be automatically added to the corresponding news message, and inserted in the rendered markdown. 
- 🛰️: Add message to the third-party section

Those emojis are just an example, you can configure them as you want in the `config.toml` file. 

#### "Admin" room
In this closed room administrative commands can be executed.

| Command         | Description                                                                |
| --------------- | -------------------------------------------------------------------------- |
| !about          | Shows bot version details                                                  |
| !clear          | Clears all stored news                                                     |
| !details "term" | Shows section/project details (term can be emoji or name)                  |
| !list-config    | Lists current bot configuration                                            |
| !list-projects  | Lists configured projects                                                  |
| !list-sections  | Lists configured sections                                                  |
| !render         | Creates a markdown file with the stored news                               |
| !restart        | Restarts the bot, useful when you edited the configuration                 |
| !say "message"  | Sends a message in reporting room                                          |
| !status         | Shows saved messages                                                       |
| !update-config  | Updates the bot configuration by executing `update_config_command` command |

### Configuration
In order to use the bot, two configuration files are required. The `config.toml` configuration file contains the bot settings (username, room ids, etc) and the definitions for the sections and projects. The second configuration file `template.md` serves as a template for the actual summary.

The password for the bot is supplied via the `BOT_PASSWORD` environment variable. Setting this will depend on how you start the bot. 

For both configuration files, examples are available that can be used as templates (see `example_config` folder). 

More configuration examples:
- [This Week in GNOME](https://gitlab.gnome.org/World/twig/-/tree/main/hebbot)

### Deployment
The bot is available as [docker image](https://hub.docker.com/r/haeckerfelix/hebbot).
You can find an example `docker-compose.yml` inside the `example_config` folder. Be sure to set the `BOT_PASSWORD` variable correctly before using it.
