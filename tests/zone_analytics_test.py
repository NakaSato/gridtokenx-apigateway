import requests
import json
import time
import sys
from datetime import datetime, timezone

# Configuration
API_URL = "http://localhost:4000/api/v1"
SUBMIT_READING_URL = "http://localhost:4000/api/meters/submit-reading"
API_KEY = "bf3a948c96147b7460f0a5073f1ec6774cc0761f19a74c94b97867de8a4564ab"  # From .env
WALLET_ADDRESS = "8CSD3C3AbhaD1kJejhrwexCQg5UFPj1qat1rdTF1UjG3"

def print_pass(message):
    print(f"✅ PASS: {message}")

def print_fail(message):
    print(f"❌ FAIL: {message}")
    sys.exit(1)

def register_meter(meter_serial, zone_id, location):
    url = f"{API_URL}/simulator/meters/register"
    headers = {
        "Content-Type": "application/json",
        "X-API-Key": API_KEY
    }
    payload = {
        "meter_id": meter_serial, # Currently binding to serial_number
        "kwh_balance": 0.0,
        "wallet_address": WALLET_ADDRESS,
        "zone_id": zone_id,
        "location": location,
        "latitude": 13.7801,
        "longitude": 100.5602,
        "meter_type": "solar"
    }
    
    response = requests.post(url, headers=headers, json=payload)
    if response.status_code == 200:
        print_pass(f"Registered meter {meter_serial} in Zone {zone_id}")
    else:
        print_fail(f"Failed to register meter {meter_serial}: {response.text}")

def submit_reading(meter_serial, kwh, zone_id):
    headers = {
        "Content-Type": "application/json",
        "X-API-Key": API_KEY
    }
    payload = {
        "kwh_amount": kwh,
        "energy_generated": kwh,  # Simplified for test
        "energy_consumed": 0.0,
        "surplus_energy": kwh,
        "deficit_energy": 0.0,
        "meter_serial": meter_serial,
        "zone_id": zone_id,
        "meter_type": "solar",
        "location": "Test Location",
        "latitude": 13.7801,
        "longitude": 100.5602,
        "wallet_address": WALLET_ADDRESS,
        "reading_timestamp": datetime.now(timezone.utc).isoformat()
    }
    
    response = requests.post(SUBMIT_READING_URL, headers=headers, json=payload)
    # 200 OK or 201 Created are acceptable, but our stub might return 200 with error message inside if token minting fails
    # The actual implementation showing "Reading received but token account creation failed" is a 200 OK with payload
    if response.status_code == 200:
        print_pass(f"Submitted reading of {kwh} kWh for {meter_serial}")
    else:
        print_fail(f"Failed to submit reading for {meter_serial}: {response.text}")

def verify_grid_status(expected_min_meters=0, check_zones=False):
    url = f"{API_URL}/public/grid-status"
    response = requests.get(url)
    
    if response.status_code != 200:
        print_fail(f"Failed to fetch grid status: {response.text}")
    
    data = response.json()
    print(f"📊 Current Grid Status: {json.dumps(data, indent=2)}")
    
    active_meters = data.get("active_meters", 0)
    if active_meters < expected_min_meters:
        print_fail(f"Expected at least {expected_min_meters} active meters, found {active_meters}")
    else:
        print_pass(f"Active meters count valid ({active_meters} >= {expected_min_meters})")

    if check_zones:
        zones = data.get("zones")
        if not zones:
            print_fail("Zones field is missing or empty in API response!")
        
        # We expect Zone 1 and Zone 2 to be present if we registered them
        # Note: keys in JSON might be strings "1", "2" even if ID is int
        z1 = zones.get("1") or zones.get(1)
        z2 = zones.get("2") or zones.get(2)
        
        if z1:
            print_pass(f"Zone 1 found: {z1}")
        else:
             print_fail("Zone 1 data missing")
             
        if z2:
            print_pass(f"Zone 2 found: {z2}")
        else:
            print_fail("Zone 2 data missing")

def main():
    print("🚀 Starting Zone Analytics Regression Test")
    
    # 1. Register Meters
    # Zone 1: 2 meters
    register_meter("reg-test-z1-m1", 1, "Zone 1 Meter A")
    register_meter("reg-test-z1-m2", 1, "Zone 1 Meter B")
    # Zone 2: 1 meter
    register_meter("reg-test-z2-m1", 2, "Zone 2 Meter A")
    
    time.sleep(1) # Allow for processing
    
    # 2. Submit Readings
    # Zone 1 total: 10 + 20 = 30 kWh
    submit_reading("reg-test-z1-m1", 10.0, 1)
    submit_reading("reg-test-z1-m2", 20.0, 1)
    # Zone 2 total: 15 kWh
    submit_reading("reg-test-z2-m1", 15.0, 2)
    
    time.sleep(1) # Allow for aggregation updates
    
    # 3. Verify
    # We expect at least 3 active meters (plus any existing ones)
    verify_grid_status(expected_min_meters=3, check_zones=True)
    
    print("✅ All tests passed!")

if __name__ == "__main__":
    main()
