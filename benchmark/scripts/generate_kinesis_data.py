"""
This script is used to generate a stream of fake stock ticker symbol data for testing purposes.
It creates simulated price movements for various stock symbols and sends them to a Kinesis stream.
"""

import datetime
import json
import random
import time
import boto3

STREAM_NAME = "ExampleInputStream"
NUM_RECORDS = 100

# Initial prices for each ticker
BASE_PRICES = {
    "AAPL": 175.50,
    "AMZN": 145.20,
    "MSFT": 370.30,
    "INTC": 45.80,
    "GOOGL": 135.90,
    "META": 345.60,
    "NVDA": 480.30,
    "AMD": 140.20,
    "TSLA": 250.90,
    "ORCL": 100.40,
}


def get_price_movement(current_price):
    """
    Generate a realistic price movement based on the current price.
    Movement is limited to a max of 0.5% up or down.
    """
    max_movement = current_price * 0.005  # 0.5% max movement
    movement = random.uniform(-max_movement, max_movement)
    return round(current_price + movement, 2)


def get_data(prices):
    """
    Generate a single record with timestamp, ticker, and price.
    Updates the price with a realistic movement.
    """
    ticker = random.choice(list(prices.keys()))
    current_price = prices[ticker]
    new_price = get_price_movement(current_price)
    prices[ticker] = new_price  # Update the price for next time

    return {
        "event_time": datetime.datetime.now().isoformat(),
        "ticker": ticker,
        "price": new_price,
    }


def generate(stream_name, kinesis_client, num_records):
    """
    Generate and send the specified number of records to Kinesis.
    """
    prices = BASE_PRICES.copy()  # Work with a copy of the base prices
    records_sent = 0

    print(f"Generating {num_records} records for Kinesis stream '{stream_name}'...")

    while records_sent < num_records:
        data = get_data(prices)
        print(f"Sending record {records_sent + 1}/{num_records}: {data}")

        kinesis_client.put_record(
            StreamName=stream_name,
            Data=json.dumps(data),
            PartitionKey=data[
                "ticker"
            ],  # Use ticker as partition key for related records
        )

        records_sent += 1
        time.sleep(0.1)  # Small delay to avoid throttling

    print(
        f"\nSuccessfully sent {records_sent} records to Kinesis stream '{stream_name}'"
    )


if __name__ == "__main__":
    kinesis_client = boto3.client("kinesis")
    generate(STREAM_NAME, kinesis_client, NUM_RECORDS)
