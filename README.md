# hebbot

A [Matrix](matrix.org) bot which can help to generate periodic / recurrent summary blog posts (also known as "This Week in X"). 

The bot was inspired by [twim-o-matic](https://github.com/matrix-org/twim-o-matic/tree/master/data), and is developed in Rust using the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk). The focus is to make it as generic as possible so that as many projects / communities can use this bot. 

Two Matrix rooms are required to use this bot:

##### "Reporting" room

This room is open to everyone. Here people can share news at any time, which will be in the next summary. 

##### "Admin" room

In this closed room administrative commands can be executed (e.g. `!clean` to remove all saved messages, or `!render-file` to create a summary as a markdown file). A complete listing of all commands can be displayed with the `!help` command.

Contextual commands are executed in the form of emoji reactions.  For example, a particular news item can be approved by adding the "⭕" emoji as a reaction to the corresponding message in the reporting room. In the same way, news can be sorted into sections, or automatically tagged with specific project information.

### Configuration
In order to use the bot, two configuration files are required. The `config.json` configuration file contains the basic bot settings (e.g. username/password, room ids, ...) and the definitions for the sections and projects. The second configuration file `template.md` serves as a template for the actual summary, which can be generated later. 

For both configuration files, examples are available that can be used as templates (`.example` files). 

### Deployment
TODO: Insert docker steps here

### Example usage
Hebbot gets used to generate the weekly GNOME summaries ("This Week in GNOME"). More information, and usage examples can be found here: TODO - insert blog post link here.
