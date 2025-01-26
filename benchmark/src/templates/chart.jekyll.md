<h3 class="text-delta chart-title" id="{{ section.anchor_id }}">
  {{ section.title }} | 
  {%- for link in section.memory_links -%}
    {%- if link.is_current -%}
      {{ link.size }}
    {%- else -%}
      <a href="#{{ link.anchor_id }}">{{ link.size }}</a>
    {%- endif -%}
    {%- if not loop.last %} | {% endif -%}
  {%- endfor -%}
</h3>

<nav class="nav" markdown="0">
  <div class="nav-group">
    <div class="nav-group-label">{{ section.navigation.cold_start.label }}</div>
    <div class="nav-group-links">
      {% for link in section.navigation.cold_start.links %}
        {% if link.is_current %}
          <span class="nav-link current">{{ link.title }}</span>
        {% else %}
          <a href="#{{ link.anchor_id }}" class="nav-link">{{ link.title }}</a>
        {% endif %}
      {% endfor %}
    </div>
  </div>
  <div class="nav-separator"></div>
  <div class="nav-group">
    <div class="nav-group-label">{{ section.navigation.warm_start.label }}</div>
    <div class="nav-group-links">
      {% for link in section.navigation.warm_start.links %}
        {% if link.is_current %}
          <span class="nav-link current">{{ link.title }}</span>
        {% else %}
          <a href="#{{ link.anchor_id }}" class="nav-link">{{ link.title }}</a>
        {% endif %}
      {% endfor %}
    </div>
  </div>
  <div class="nav-separator"></div>
  <div class="nav-group">
    <div class="nav-group-label">{{ section.navigation.resources.label }}</div>
    <div class="nav-group-links">
      {% for link in section.navigation.resources.links %}
        {% if link.is_current %}
          <span class="nav-link current">{{ link.title }}</span>
        {% else %}
          <a href="#{{ link.anchor_id }}" class="nav-link">{{ link.title }}</a>
        {% endif %}
      {% endfor %}
    </div>
  </div>
</nav>

<div id="{{ section.chart_id }}" class="chart-container"></div>

<div class="download-data">
  <a href="{{ section.data_filename }}" download class="btn btn-blue">Download Raw Data</a>
</div>

<script src="https://cdn.jsdelivr.net/npm/echarts@5.4.3/dist/echarts.min.js"></script>
<script type="text/javascript">
    var chart = echarts.init(document.getElementById('{{ section.chart_id }}'), 'dark');
    var options = {{ section.options_json | safe }};
    chart.setOption(options);
    window.addEventListener('resize', function() {
        chart.resize();
    });
</script> 