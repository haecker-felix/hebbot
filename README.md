# hebbot

[![](https://img.shields.io/github/v/release/haecker-felix/hebbot)](https://github.com/haecker-felix/hebbot/releases)
[![](https://img.shields.io/badge/matrix-%23hebbot%3Ahaecker.io-lightgrey)](https://matrix.to/#/#hebbot:haecker.io)
[![]( https://img.shields.io/github/actions/workflow/status/haecker-felix/hebbot/build.yml)](https://github.com/haecker-felix/hebbot/actions)

A [Matrix](matrix.org) bot which can help to generate periodic / recurrent summary blog posts (also known as "This Week in X"). 

The bot was inspired by [twim-o-matic](https://github.com/matrix-org/twim-o-matic/tree/master/data), and is developed in Rust using the [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk). You can find us at [#hebbot:haecker.io](https://matrix.to/#/#hebbot:haecker.io).

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

### Example Usage
- [This Week in GNOME](https://gitlab.gnome.org/World/twig) ([configuration](https://gitlab.gnome.org/World/twig/-/tree/main/hebbot))
- [This Week in Matrix](https://matrix.org/blog/category/this-week-in-matrix) ([configuration](https://github.com/matrix-org/twim-config))
- [The Bullhorn](https://forum.ansible.com/c/news/bullhorn/17) ([configuration](https://github.com/ansible-community/ansible.im/tree/main/bots))

If you know any other project which uses Hebbot and you think it should be get listed here, please open a PR!
