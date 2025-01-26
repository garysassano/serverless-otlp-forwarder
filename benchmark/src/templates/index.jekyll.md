---
layout: default
title: {{ title }}
{% if function_title and function_title != "" %}
parent: {{ function_title }}
{% endif %}
has_children: true
nav_order: {{ nav_order }}
has_toc: false
---

# {{ title }}

{% if global_description %}
{{ global_description }}
{% endif %}

{% if metadata %}
## Configuration
{% for meta in metadata %}
- {{ meta.label }}: {{ meta.value }}
{% endfor %}
{% endif %}

{% for item in items %}
## [{{ item.title }}]({{ item.path }})
{: .text-delta }

{% if item.subtitle %}
{{ item.subtitle }}
{% endif %}

{% if item.charts %}
{% for chart in item.charts %}
{{ chart }}
{% endfor %}
{% endif %}

{% endfor %} 