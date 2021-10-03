# hebbot

A [Matrix](matrix.org) bot which can help to generate periodic / recurrent summary blog posts (also known as "This Week in X"). 

The bot was inspired by [twim-o-matic](https://github.com/matrix-org/twim-o-matic/tree/master/data), and is developed in Rust using the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk). 

### Features
- Automatic recognition of news when the bot username is mentioned at the beginning of the message
- Approval of messages by a defined group of editors
- Messages can be sorted into projects / sections by using emoji reactions
- Support for images / videos
- Markdown generation (can be used for blogs, e.g. Hugo) 

### Screenshots
![](doc/images/render_command.png)
![](doc/images/message_recognition.png)

### Documentation
Check out documentation for...
- latest [stable release v2.1](https://github.com/haecker-felix/hebbot/tree/e1f43fbadf2bd284d78c270c0fe8ef231c8a7978/doc)
- unstable [development builds](https://github.com/haecker-felix/hebbot/tree/master/doc)

### Example usage
Hebbot gets used to generate the weekly GNOME summaries ("This Week in GNOME"). More information, and usage examples can be found here: 
- https://blogs.gnome.org/haeckerfelix/2021/07/16/introducing-this-week-in-gnome/
- https://thisweek.gnome.org/

If you know any other project which uses Hebbot and you think it should be get listed here, please open a PR!
