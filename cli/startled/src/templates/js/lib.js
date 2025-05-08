/**
 * Consolidated JavaScript library for the startled CLI report generator
 * 
 * This file contains all the JavaScript functionality needed for the benchmark reports:
 * 1. Base functionality for theming, UI behavior, and chart initialization
 * 2. Bar chart generation for metrics like cold start, warm start, and memory usage
 * 3. Line chart generation for time-series data like client duration over time
 * 
 * Users can customize this file to change the appearance and behavior of the reports.
 * When providing a custom template directory with --template-dir, place a modified
 * version of this file at: <your-template-dir>/js/lib.js
 */

// Default color palette for charts (can be customized)
window.DEFAULT_COLOR_PALETTE = [
    "#3fb1e3", "#6be6c1", "#626c91",
    "#a0a7e6", "#c4ebad", "#96dee8"
];

// ===============================
// Core UI and Setup Functionality
// ===============================

// Theme and chart globals
let chart;
const root = document.documentElement;

/**
 * Sets the theme for the entire report and initializes/reinitializes the chart
 * @param {string} theme - The theme name ('light' or 'dark')
 */
function setTheme(theme) {
    root.setAttribute('data-theme', theme);
    localStorage.setItem('theme', theme);
    // Update icons if present
    const darkIcon = document.querySelector('.dark-icon');
    const lightIcon = document.querySelector('.light-icon');
    if (darkIcon && lightIcon) {
        if (theme === 'dark') {
            darkIcon.style.display = 'block';
            lightIcon.style.display = 'none';
        } else {
            darkIcon.style.display = 'none';
            lightIcon.style.display = 'block';
        }
    }
    // Reinitialize chart with new theme
    if (chart) {
        chart.dispose();
    }
    
    const chartDom = document.getElementById('chart'); // Assuming 'chart' is the consistent ID
    if (!chartDom) {
        console.error("Chart DOM element with id 'chart' not found.");
        return;
    }

    chart = echarts.init(chartDom, theme); // 'theme' for ECharts built-in themes

    let options;
    if (window.currentChartSpecificData) {
        // Check the structure to determine the chart type
        if (window.currentChartSpecificData.Bar) {
            options = BarCharts.generateOptions(window.currentChartSpecificData);
        } else if (window.currentChartSpecificData.Line) {
            options = LineCharts.generateOptions(window.currentChartSpecificData);
        } else {
            console.error('Unknown chart data format:', window.currentChartSpecificData);
        }

        // Apply the default color palette if the generator didn't set one
        if (options && typeof options.color === 'undefined' && window.DEFAULT_COLOR_PALETTE) {
            options.color = window.DEFAULT_COLOR_PALETTE;
        }

        if (options) {
            chart.setOption(options);
        } else {
             console.error("Failed to generate chart options.");
        }
    } else {
        // This is the error we expect if the data file didn't load/execute correctly
        console.error("window.currentChartSpecificData is not defined. Cannot set chart options.");
    }
}

/**
 * Prepares the page for taking screenshots
 * @param {string} theme - The theme to use for the screenshot
 */
function prepareScreenshot(theme) {
    setTheme(theme);
    // Hide sidebar and adjust layout for screenshots
    const sidebar = document.querySelector('.sidebar');
    const mainContent = document.querySelector('.main-content');
    const sidebarToggle = document.querySelector('.sidebar-toggle');
    if (sidebar) sidebar.style.display = 'none';
    if (mainContent) {
        mainContent.style.marginLeft = '0';
        mainContent.style.width = '100%';
        mainContent.style.maxWidth = '100%';
    }
    if (sidebarToggle) sidebarToggle.style.display = 'none';
    // Resize chart after DOM updates
    setTimeout(() => {
        if (chart) {
            chart.resize();
        }
    }, 200);
}

// ============================
// Bar Chart Generator Module
// using Apache echarts.js (https://echarts.apache.org/en/index.html)
// ============================

/**
 * Module for generating bar chart options
 * Used for visualizing metrics like cold start times, memory usage, etc.
 */
const BarCharts = {
    /**
     * Generates ECharts options for bar charts
     * @param {Object} chartSpecificData - The chart data from the server
     * @returns {Object} ECharts options object
     */
    generateOptions: function(chartSpecificData) {
        if (!chartSpecificData || !chartSpecificData.Bar) {
            console.error("Invalid data format for bar chart generator:", chartSpecificData);
            return {}; // Return empty options on error
        }
        const data = chartSpecificData.Bar;

        const echartsSeries = data.series.map(s => ({
            name: s.name,
            type: 'bar',
            label: {
                show: true,
                position: 'right',
                formatter: `{c} ${data.unit}`
            },
            data: s.values.map((value, index) => ({
                value: value,
                name: data.y_axis_categories[index] // Assumes values align with categories
            }))
        }));

        const options = {
            title: {
                text: data.title.toUpperCase(),
                top: "5",
                left: "center",
                textStyle: { fontWeight: "light", color: "#666" }
            },
            tooltip: { 
                trigger: "axis", 
                axisPointer: { type: "shadow" } 
            },
            // Color palette will be applied by lib.js 
            legend: {
                orient: "horizontal",
                bottom: 5,
                type: "scroll"
            },
            grid: [{
                left: "10%", top: "15%", right: "15%", bottom: "10%",
                containLabel: true
            }],
            xAxis: [{
                type: "value",
                name: `${data.unit === "MB" ? "Memory" : "Duration"} (${data.unit})`,
                nameLocation: "middle",
                nameGap: 30,
                axisLabel: { formatter: `{value} ${data.unit}` },
                minInterval: 1
            }],
            yAxis: [{
                type: "category",
                inverse: true,
                data: data.y_axis_categories
            }],
            series: echartsSeries,
            toolbox: {
                feature: { restore: {}, saveAsImage: {} },
                right: "20px"
            },
            // Base responsive design (can be customized further in templates)
            media: [
                {
                    query: { maxWidth: 768 },
                    option: {
                        legend: {
                            top: "auto",
                            bottom: 5,
                            orient: "horizontal"
                        },
                        grid: [{
                            left: "5%",
                            right: "8%",
                            top: "10%",
                            bottom: "18%" 
                        }],
                        xAxis: [{
                            nameGap: 20,
                            axisLabel: { fontSize: 10 },
                            nameTextStyle: { fontSize: 11 }
                        }],
                        yAxis: [{
                            axisLabel: { fontSize: 10 },
                            nameTextStyle: { fontSize: 11 }
                        }]
                    }
                }
            ]
        };

        return options;
    }
};

// ============================
// Line Chart Generator Module
// ============================

/**
 * Module for generating line/scatter chart options
 * Used for time-series data like client duration over time
 */
const LineCharts = {
    /**
     * Generates ECharts options for line/scatter charts
     * @param {Object} chartSpecificData - The chart data from the server
     * @returns {Object} ECharts options object
     */
    generateOptions: function(chartSpecificData) {
        if (!chartSpecificData || !chartSpecificData.Line) {
            console.error("Invalid data format for line chart generator:", chartSpecificData);
            return {}; // Return empty options on error
        }
        const data = chartSpecificData.Line;

        // Determine y-axis max based on P90 of all points
        const allYValues = data.series.flatMap(s => s.points.map(p => p.y));
        let yMax = 1000; // Default max
        if (allYValues.length > 0) {
            // Simple P90 calculation (sort and pick) - might need a more robust library for large datasets
            allYValues.sort((a, b) => a - b);
            const p90Index = Math.floor(allYValues.length * 0.9);
            const p90 = allYValues[p90Index];
            yMax = p90 * 1.2; // Add some headroom
        }

        // Transform series data for ECharts
        const echartsSeries = data.series.map(s => {
            const seriesPoints = s.points.map(p => ({
                value: [p.x, p.y] // ECharts scatter data format [x, y]
            }));

            const markLineData = [];
            if (s.mean !== null && s.mean !== undefined) {
                 const lastPointX = s.points.length > 0 ? s.points[s.points.length - 1].x : s.points[0]?.x ?? 0; // Find max x for this series
                 const firstPointX = s.points[0]?.x ?? 0;
                 markLineData.push(
                     // Mean line (Note: ECharts markLine is somewhat limited for scatter plots)
                     // We draw a simple horizontal line using yAxis value.
                     // For a line spanning just the series points, more complex logic or a different
                     // approach (like adding a separate 'line' series) might be needed.
                     {
                         name: `${s.name} Mean`,
                         yAxis: s.mean,
                         // Attempting to constrain line - might not work perfectly in scatter
                         // xAxis: lastPointX, 
                         label: {
                             show: true,
                             formatter: `{c} ${data.unit}`, // Use unit from data
                             position: 'end',
                             // Color will be inherited
                         },
                     },
                     // Add trendline if needed (more complex)
                 );
            }

            return {
                name: s.name,
                type: 'scatter',
                // smooth: true, // Not applicable to scatter
                showSymbol: true, // Show points
                symbolSize: 6, // Adjust point size if needed
                label: { show: false }, // Generally too noisy for scatter
                data: seriesPoints,
                markLine: {
                    silent: true, // Non-interactive
                    symbol: ["none", "none"], // No arrows
                    lineStyle: {
                        // Color is inherited
                        width: 2,
                        type: "dashed"
                    },
                    data: markLineData
                }
            };
        });
        
        // Filter legend data to exclude "Mean" lines if they were separate series
        const legendData = data.series.map(s => s.name);


        const options = {
            title: {
                text: data.title.toUpperCase(),
                top: "5",
                left: "center",
                textStyle: { fontWeight: "light", color: "#666" }
            },
            tooltip: { 
                trigger: "axis", // Or 'item' for scatter points
                axisPointer: { type: "cross" } 
            },
            // Color palette will be applied by lib.js
            grid: {
                top: "10%", bottom: "10%", left: "8%", right: "9%", containLabel: true
            },
            legend: {
                data: legendData, // Use filtered legend names
                bottom: 5,
                orient: "horizontal",
                type: "scroll"
            },
            xAxis: {
                type: "value",
                name: data.x_axis_label,
                nameLocation: "middle",
                nameGap: 30,
                min: 0,
                max: data.total_x_points + 1, // Use max calculated in Rust
                minInterval: 1,
                boundaryGap: false,
                splitLine: { show: false }
            },
            yAxis: {
                type: "value",
                name: data.y_axis_label,
                nameLocation: "middle",
                nameGap: 50,
                splitLine: { show: true },
                max: yMax, // Use calculated P90-based max
                axisLabel: {
                    formatter: `{value} ${data.unit}`
                }
            },
            series: echartsSeries,
            toolbox: {
                feature: { restore: {}, saveAsImage: {} },
                right: "20px"
            },
             // Base responsive design (can be customized further in templates)
            media: [
                {
                    query: { maxWidth: 768 },
                    option: {
                        legend: {
                            top: "auto",
                            bottom: 5,
                            orient: "horizontal"
                        },
                        grid: {
                            top: "15%",
                            bottom: "18%",
                            left: "10%", 
                            right: "8%"
                        },
                        xAxis: {
                             nameGap: 20,
                             axisLabel: { fontSize: 10 },
                             nameTextStyle: { fontSize: 11 }
                        },
                         yAxis: {
                             nameGap: 35,
                             axisLabel: { fontSize: 10 },
                             nameTextStyle: { fontSize: 11 }
                        }
                    }
                }
            ]
        };

        return options;
    }
};

// ======================
// Initialization on Load
// ======================

// DOMContentLoaded handler
window.addEventListener('DOMContentLoaded', () => {
    // Initialize theme from localStorage or system preference
    const savedTheme = localStorage.getItem('theme');
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const initialTheme = savedTheme || (prefersDark ? 'dark' : 'light');
    setTheme(initialTheme);

    // Theme toggle handler
    const themeToggle = document.querySelector('.theme-toggle');
    if (themeToggle) {
        themeToggle.addEventListener('click', () => {
            const currentTheme = root.getAttribute('data-theme');
            setTheme(currentTheme === 'dark' ? 'light' : 'dark');
        });
    }

    // Sidebar toggle
    const sidebar = document.getElementById('sidebar');
    const toggleButton = document.getElementById('sidebar-toggle');
    if (toggleButton && sidebar) {
        toggleButton.addEventListener('click', () => {
            sidebar.classList.toggle('sidebar-open');
        });
    }

    // Window resize handler
    window.addEventListener('resize', function() {
        if (chart) {
            chart.resize();
        }
    });

    // Navigation handler (if needed)
    window.navigateToChartType = function(event) {
        event.preventDefault();
        const linkElement = event.currentTarget;
        const targetGroup = linkElement.dataset.group;
        const targetSubgroup = linkElement.dataset.subgroup;
        // Get the current chart type or default to cold-start-init
        const currentChartType = window.currentChartType || 'cold-start-init';
        // Use basePath if available, otherwise fallback to root
        const basePath = window.basePath || '/';
        // Construct URL with proper base path and trailing slash
        const newUrl = basePath + targetGroup + '/' + targetSubgroup + '/' + currentChartType + '/';
        window.location.href = newUrl;
    };
});

// Expose functions globally
window.setTheme = setTheme;
window.prepareScreenshot = prepareScreenshot; 