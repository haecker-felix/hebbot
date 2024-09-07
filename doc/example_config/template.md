---
title: "#{{ now() | dateformat(format='[week_number]') }}: This Week in X"
author: {{ editor }}
date: {{ now() | datetimeformat(format="iso") }}
tags: {{ projects }}
categories: ["weekly-update"]
draft: false
---
{#-
 # Here are some pointers to get started writing custom templates:
 #
 # - This template is processed using MiniJinja:
 #   https://docs.rs/minijinja/latest/minijinja/
 #
 # - Template syntax is mostly compatible with Jinja2:
 #   https://jinja.palletsprojects.com/en/3.1.x/templates/
 #
 # - Date formatting is done using time.rs format specifiers:
 #   https://time-rs.github.io/book/api/format-description.html
 #
 # - Adding {{ debug() }} will insert the contents of the environment
 #   used for processing the template. This is useful for writing custom
 #   templates.
 #
 # - Macros can be used to avoid repeating template fragments. See below
 #   for an example macro to handle both section and project news.
 #
 # - Hebbot will detect when the template has changed on disk and reload
 #   the file contents the next time it receives a !render command.
-#}

{%- macro news(news_items) -%}
  {%- for item in news_items %}

[{{ item.reporter_display_name }}](https://matrix.to/#{{ item.reporter_id }}) {{ config.verbs | random }}

> {{ item.message | replace("\n", "\n> ") }}
    {%- if item.images -%}
      {%- for imgId, image in item.images | dictsort %}
> ![]({{ image[0] }})
      {%- endfor %}
    {%- endif -%} {#- news item images #}

    {%- if item.videos -%}
      {%- for vidId, video in item.videos | dictsort %}
> {{ "{{" }}<video src="{{ video[0] }}">{{ "}}" }}
      {%- endfor %}
    {%- endif -%} {#- news item videos #}

  {%- endfor %} {#- news_items #}
{%- endmacro %}

Update on what happened across the X project in the week from {{timespan}}.

{%- for key, entry in sections | dictsort %}

## {{entry.section.title}} {{entry.section.emoji}}

  {{- news(entry.news) }}

  {%- for entry in entry.projects %}

### {{ entry.project.title}} [↗]({{ entry.project.website }}) {{ entry.project.emoji }}

{{ entry.project.description }}

    {{- news(entry.news) }}

  {%- endfor %} {#- projects #}

{%- endfor %} {#- sections #}

# That’s all for this week!

See you next week!
