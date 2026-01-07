
import requests
import json
import time
import random
import uuid

# Configuration
import subprocess
import os

# Configuration
BASE_URL = "http://localhost:4000/api/v1"
SELLER_EMAIL = f"seller_{random.randint(1000,9999)}@test.com"
BUYER_EMAIL = f"buyer_{random.randint(1000,9999)}@test.com"
PASSWORD = "StrongP@ssw0rd!2025"

def get_env_var(key):
    try:
        with open("../.env", "r") as f:
            for line in f:
                if line.startswith(f"{key}="):
                    return line.strip().split("=")[1]
    except Exception:
        pass
    return None

CURRENCY_MINT = get_env_var("CURRENCY_TOKEN_MINT")


class GridTokenXClient:
    def __init__(self, email, password, role="user"):
        self.email = email
        self.password = password
        self.role = role
        self.token = None
        self.user_id = None
        self.wallet = None
        self.meter_serial = None
        self.meter_id = None

    def register(self):
        print(f"[{self.email}] Registering...")
        url = f"{BASE_URL}/users"
        data = {
            "username": self.email.split("@")[0],
            "email": self.email,
            "password": self.password,
            "first_name": "Test",
            "last_name": "User"
        }
        resp = requests.post(url, json=data)
        if resp.status_code == 200 or resp.status_code == 201:
            print(f"[{self.email}] Registered successfully.")
        else:
            print(f"[{self.email}] Registration failed: {resp.text}")
            # Try login if already exists
            
    def login(self):
        print(f"[{self.email}] Logging in...")
        url = f"{BASE_URL}/auth/token"
        data = {
            "username": self.email,
            "password": self.password
        }
        resp = requests.post(url, json=data)
        if resp.status_code == 200:
            auth_data = resp.json()
            self.token = auth_data["access_token"]
            self.user_id = auth_data["user"]["id"]
            self.wallet = auth_data["user"].get("wallet_address")
            print(f"[{self.email}] Login success. Wallet: {self.wallet}")
        else:
            print(f"[{self.email}] Login failed: {resp.text}")
            exit(1)

    def register_meter(self):
        print(f"[{self.email}] Registering meter...")
        self.meter_serial = f"SERIAL-{uuid.uuid4().hex[:8].upper()}"
        url = f"{BASE_URL}/meters"
        headers = {"Authorization": f"Bearer {self.token}"}
        data = {
            "serial_number": self.meter_serial,
            "meter_type": "Solar_Prosumer",
            "location": "Test Location",
            "latitude": 13.7563,
            "longitude": 100.5018
        }
        resp = requests.post(url, json=data, headers=headers)
        if resp.status_code == 200 or resp.status_code == 201:
            meter_data = resp.json()
            self.meter_id = meter_data["meter"]["id"] if "meter" in meter_data else None
            print(f"[{self.email}] Meter registered: {self.meter_serial}")
        else:
            print(f"[{self.email}] Meter registration failed: {resp.text}")
            exit(1)

    def submit_reading(self, kwh, auto_mint=False):
        print(f"[{self.email}] Submitting reading for {kwh} kWh (AutoSprint={auto_mint})...")
        url = f"{BASE_URL}/meters/{self.meter_serial}/readings"
        headers = {"Authorization": f"Bearer {self.token}"}
        params = {"auto_mint": str(auto_mint).lower()}
        data = {
            "kwh": kwh,
             # Request requires float, ensuring it works
        }
        resp = requests.post(url, json=data, headers=headers, params=params)
        if resp.status_code == 200 or resp.status_code == 201:
            reading = resp.json()
            print(f"[{self.email}] Reading submitted. Response: {json.dumps(reading)}")
            return reading
        else:
            print(f"[{self.email}] Reading submission failed: {resp.text}")
            exit(1)

    def mint_reading(self, reading_id):
        print(f"[{self.email}] Minting reading {reading_id}...")
        url = f"{BASE_URL}/meters/readings/{reading_id}/mint"
        headers = {"Authorization": f"Bearer {self.token}"}
        resp = requests.post(url, headers=headers)
        if resp.status_code == 200:
            print(f"[{self.email}] Mint success: {resp.json().get('transaction_signature')}")
            return True
        else:
            print(f"[{self.email}] Mint failed: {resp.text}")
            return False

    def create_order(self, side, amount, price):
        print(f"[{self.email}] Creating {side} Limit order: {amount} kWh @ {price} GRX...")
        url = f"{BASE_URL}/trading/orders"
        headers = {"Authorization": f"Bearer {self.token}"}
        data = {
            "side": side.lower(),
            "order_type": "limit",
            "energy_amount": amount,
            "price_per_kwh": price,
            "zone_id": 1
        }
        resp = requests.post(url, json=data, headers=headers)
        if resp.status_code == 200 or resp.status_code == 201:
            order_data = resp.json()
            # print(f"Order Creation Response: {order_data}")
            order_id = order_data.get("id", "unknown") 
            print(f"[{self.email}] Order created. ID: {order_id}")
            return order_id
        else:
            print(f"[{self.email}] Order creation failed: {resp.text}")
            exit(1)

    def get_profile(self):
        url = f"{BASE_URL}/users/me"
        headers = {"Authorization": f"Bearer {self.token}"}
        resp = requests.get(url, headers=headers)
        if resp.status_code == 200:
            user_data = resp.json()
            self.wallet = user_data.get("wallet_address")
            print(f"[{self.email}] Profile refreshed. Wallet: {self.wallet}")
        else:
            print(f"[{self.email}] Failed to get profile: {resp.text}")

    def setup_wallet(self):
        print(f"[{self.email}] Setting up wallet (via dummy order)...")
        # Create a dummy buy order to trigger wallet generation
        # We use a small amount and price to ensure it passes basic validation
        url = f"{BASE_URL}/trading/orders"
        headers = {"Authorization": f"Bearer {self.token}"}
        data = {
            "side": "buy",
            "order_type": "limit",
            "energy_amount": 1.0,
            "price_per_kwh": 1.0,
            "zone_id": 1
        }
        # We expect this might succeed or fail, but the side effect is wallet generation
        resp = requests.post(url, json=data, headers=headers)
        # print(f"Dummy order response: {resp.status_code} {resp.text}")
        # Refresh profile to get wallet
        self.get_profile()

    def get_my_orders(self):
        url = f"{BASE_URL}/trading/orders"
        headers = {"Authorization": f"Bearer {self.token}"}
        resp = requests.get(url, headers=headers)
        if resp.status_code == 200:
            json_resp = resp.json()
            return json_resp.get("data", [])
        return []

def main():
    print("=== Starting P2P Verification ===")

    # 1. Setup Seller
    seller = GridTokenXClient(SELLER_EMAIL, PASSWORD)
    seller.register()
    seller.login()
    seller.setup_wallet() # Generate wallet
    seller.register_meter()
    
    # 2. Setup Buyer
    buyer = GridTokenXClient(BUYER_EMAIL, PASSWORD)
    buyer.register()
    buyer.login()
    buyer.setup_wallet() # Generate wallet
    
    # Buyer needs tokens too? No, buyer buys Energy Tokens using what? 
    # In this system, orders are created.
    
    # 3. Seller Mints Tokens
    # Submit reading
    # Now that seller has wallet, reading submission should work
    reading = seller.submit_reading(50.0, auto_mint=False)
    time.sleep(1)
    
    # Mint manually
    success = seller.mint_reading(reading["id"])
    if not success:
        print("!!! Setup failed: Could not mint tokens for seller.")
        exit(1)
        
    print("Waiting for mint confirmation...")
    time.sleep(5) 

    # 3b. Fund Buyer with Currency
    if CURRENCY_MINT:
        print(f"[{buyer.email}] Funding with Currency ({CURRENCY_MINT})...")
        try:
             # Ensure buyer has ATA
            subprocess.run(
                ["spl-token", "create-account", CURRENCY_MINT, "--owner", buyer.wallet, "--fee-payer", "dev-wallet.json"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=".."
            )
            # Mint tokens
            subprocess.run(
                ["spl-token", "mint", CURRENCY_MINT, "1000", buyer.wallet, "--fee-payer", "dev-wallet.json", "--mint-authority", "dev-wallet.json"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=".."
            )
            print(f"[{buyer.email}] Funded with 1000 Currency Tokens.")
        except Exception as e:
            print(f"Failed to fund buyer: {e}")
    else:
        print("WARNING: CURRENCY_TOKEN_MINT not found in .env, buyer might fail to lock escrow.")
    
    # 4. Create Orders
    price = 2.0
    amount = 10.0
    
    # Seller Sells
    seller_order_id = seller.create_order("sell", amount, price)
    
    # Buyer Buys
    buyer_order_id = buyer.create_order("buy", amount, price)
    
    # 5. Wait for matching
    print("Waiting for matching engine (15s)...")
    time.sleep(15)
    
    # 6. Check status
    print("Checking Seller Orders...")
    seller_orders = seller.get_my_orders()
    # print(f"Seller Orders Dump: {seller_orders}")
    
    for o in seller_orders:
        # Check if o is dict
        if isinstance(o, dict) and o.get("id") == seller_order_id:
            print(f"Seller Order Status: {o.get('status')} (Filled: {o.get('filled_amount')})")
        elif isinstance(o, str) and o == seller_order_id:
             print(f"Seller Order Found (ID only): {o}")
            
    print("Checking Buyer Orders...")
    buyer_orders = buyer.get_my_orders()
    for o in buyer_orders:
        if isinstance(o, dict) and o.get("id") == buyer_order_id:
            print(f"Buyer Order Status: {o.get('status')} (Filled: {o.get('filled_amount')})")
        elif isinstance(o, str) and o == buyer_order_id:
             print(f"Buyer Order Found (ID only): {o}")

    # TODO: Check settlements endpoint if available or database
    
    print("=== Verification Complete ===")

if __name__ == "__main__":
    main()
