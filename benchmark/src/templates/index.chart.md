---
layout: default
title: {{ title }}
{% if function_title and function_title != "" %}
parent: {% if global_title %}{{ global_title }}{% else %}Benchmark Results{% endif %}
{% endif %}
has_children: true
nav_order: {{ nav_order }}
has_toc: false
---

# {{ title }}

{% if description %}
{{ description }}
{% endif %}

{% if metadata %}
<div class="benchmark-config" markdown="0">
  <h2>Configuration</h2>
  <div class="config-grid">
    {% for meta in metadata %}
    <div class="config-item">
      <span class="config-label">{{ meta.label }}:</span>
      <span class="config-value">{{ meta.value }}</span>
    </div>
    {% endfor %}
  </div>
</div>
{% endif %}

{% for item in items %}

{% if item.subtitle %}
{{ item.subtitle }}
{% endif %}

{% if item.charts %}
{% for chart in item.charts %}
{{ chart }}
{% endfor %}
{% endif %}
{% endfor %} 