## Cost Comparison: Different Telemetry Approaches vs OpenTelemetry Collector

Provide a cost comparison for Cloudwatch ingestion, or the Kinesis experimental extension,
vs the increased billed duration due to execution overhead when using a sidecar collector.

### Assumptions
* Base Lambda execution time: 100ms
* Lambda memory: 1.0GB
* Lambda cost: $1.66667e-05 per GB-second
* CloudWatch Logs ingestion cost: $0.5 per GB
* Kinesis Data Streams ingestion cost (on-demand): $0.08 per GB
* Kinesis Data Streams stream cost: Not included (negligible at scale)

## SCENARIO 1: CloudWatch Logs vs OTel Collector

### Approaches being compared
1. **CloudWatch Logs approach**: Write directly to CloudWatch Logs
   * Lambda runs at base duration
   * Incurs CloudWatch Logs ingestion costs
2. **OTel Collector approach**: Use OpenTelemetry Collector
   * Lambda runs with execution overhead (columns)
   * No CloudWatch Logs ingestion costs

## Raw Cost Ratios (CloudWatch / OTel Collector)
* Values > 100%: CloudWatch is more expensive (OTel Collector cheaper)
* Values < 100%: CloudWatch is cheaper (OTel Collector more expensive)

| Payload Size | 1x | 1.5x | 2x | 3x | 4x | 5x | 6x | 7x | 8x | 9x | 10x |
|-|-|-|-|-|-|-|-|-|-|-|-|
| 1KB | 129% | 86% | 64% | 43% | 32% | 26% | 21% | 18% | 16% | 14% | 13% |
| 4KB | 214% | 143% | 107% | 71% | 54% | 43% | 36% | 31% | 27% | 24% | 21% |
| 8KB | 329% | 219% | 164% | 110% | 82% | 66% | 55% | 47% | 41% | 37% | 33% |
| 16KB | 558% | 372% | 279% | 186% | 139% | 112% | 93% | 80% | 70% | 62% | 56% |

## Cost Comparison: CloudWatch vs OTel Collector
* **Rows**: Payload sizes in KB
* **Columns**: OTel Collector execution overhead factor
* **Values**: How many times cheaper one approach is versus the other


Heatmap visualization saved as 'png/cloudwatch_vs_otel_heatmap.png'

## Example Calculations: CloudWatch vs OTel Collector

### Example: 1KB payload with 1.5x OTel Collector execution overhead

**CloudWatch Logs approach:**
* Lambda execution: `$0.00000167` (100ms at 1.0GB)
* Log ingestion: `$0.00000048` (1KB at $0.5/GB)
* **Total: `$0.00000214`**

**OTel Collector approach:**
* Lambda execution: `$0.00000250` (150.0ms at 1.0GB)
* **Total: `$0.00000250`**

**Result:**
* **CloudWatch is 1.17x cheaper**

### Example: 16KB payload with 8x OTel Collector execution overhead

**CloudWatch Logs approach:**
* Lambda execution: `$0.00000167` (100ms at 1.0GB)
* Log ingestion: `$0.00000763` (16KB at $0.5/GB)
* **Total: `$0.00000930`**

**OTel Collector approach:**
* Lambda execution: `$0.00001333` (800ms at 1.0GB)
* **Total: `$0.00001333`**

**Result:**
* **CloudWatch is 1.43x cheaper**
## SCENARIO 2: Kinesis Data Streams vs OTel Collector

### Approaches being compared
1. **Kinesis Data Streams approach**: Send payload to Kinesis
   * Lambda runs at base duration
   * Incurs Kinesis Data Streams ingestion costs
   * Kinesis stream costs not included (negligible at scale)
2. **OTel Collector approach**: Use OpenTelemetry Collector
   * Lambda runs with execution overhead (columns)
   * No Kinesis Data Streams costs

## Raw Cost Ratios (Kinesis / OTel Collector)
* Values > 100%: Kinesis is more expensive (OTel Collector cheaper)
* Values < 100%: Kinesis is cheaper (OTel Collector more expensive)

| Payload Size | 1x | 1.5x | 2x | 3x | 4x | 5x | 6x | 7x | 8x | 9x | 10x |
|-|-|-|-|-|-|-|-|-|-|-|-|
| 1KB | 105% | 70% | 52% | 35% | 26% | 21% | 17% | 15% | 13% | 12% | 10% |
| 4KB | 118% | 79% | 59% | 39% | 30% | 24% | 20% | 17% | 15% | 13% | 12% |
| 8KB | 137% | 91% | 68% | 46% | 34% | 27% | 23% | 20% | 17% | 15% | 14% |
| 16KB | 173% | 115% | 87% | 58% | 43% | 35% | 29% | 25% | 22% | 19% | 17% |

## Cost Comparison: Kinesis vs OTel Collector
* **Rows**: Payload sizes in KB
* **Columns**: OTel Collector execution overhead factor
* **Values**: How many times cheaper one approach is versus the other


Heatmap visualization saved as 'png/kinesis_vs_otel_heatmap.png'

## Example Calculations: Kinesis vs OTel Collector

### Example: 1KB payload with 1.5x OTel Collector execution overhead

**Kinesis Data Streams approach:**
* Lambda execution: `$0.00000167` (100ms at 1.0GB)
* Data ingestion: `$0.00000008` (1KB at $0.08/GB)
* Stream cost: Not included (negligible at scale)
* **Total: `$0.00000174`**

**OTel Collector approach:**
* Lambda execution: `$0.00000250` (150.0ms at 1.0GB)
* **Total: `$0.00000250`**

**Result:**
* **Kinesis is 1.43x cheaper**

### Example: 16KB payload with 8x OTel Collector execution overhead

**Kinesis Data Streams approach:**
* Lambda execution: `$0.00000167` (100ms at 1.0GB)
* Data ingestion: `$0.00000122` (16KB at $0.08/GB)
* Stream cost: Not included (negligible at scale)
* **Total: `$0.00000289`**

**OTel Collector approach:**
* Lambda execution: `$0.00001333` (800ms at 1.0GB)
* **Total: `$0.00001333`**

**Result:**
* **Kinesis is 4.62x cheaper**
