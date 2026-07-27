# Frontend Integration Guide

Complete guide for integrating TrustLink escrow contracts into your frontend application.

## Table of Contents
- [Installation](#installation)
- [Setup](#setup)
- [Wallet Connection](#wallet-connection)
- [Contract Calls](#contract-calls)
- [Event Listening](#event-listening)
- [Error Handling](#error-handling)
- [Complete Examples](#complete-examples)

## Installation

```bash
npm install @stellar/stellar-sdk stellar-wallets-kit
# or
yarn add @stellar/stellar-sdk stellar-wallets-kit
```

## Setup

### Initialize Stellar SDK

```typescript
import { SorobanRpc, Contract, Networks, TransactionBuilder } from '@stellar/stellar-sdk';

// Configure RPC server
const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');

// Contract ID (replace with your deployed contract)
const CONTRACT_ID = 'CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX';

// Initialize contract
const contract = new Contract(CONTRACT_ID);
```

## Wallet Connection

### Using Freighter Wallet

```typescript
import { isConnected, getPublicKey, signTransaction } from '@stellar/freighter-api';

async function connectWallet(): Promise<string> {
  // Check if Freighter is installed
  const hasFreighter = await isConnected();
  if (!hasFreighter) {
    throw new Error('Please install Freighter wallet');
  }

  // Get user's public key
  const publicKey = await getPublicKey();
  return publicKey;
}
```

### Using Hardware Wallets (Ledger)

```typescript
import TransportWebUSB from '@ledgerhq/hw-transport-webusb';
import Stellar from '@ledgerhq/hw-app-str';

async function connectLedger(): Promise<{ publicKey: string; sign: Function }> {
  // Connect to Ledger device
  const transport = await TransportWebUSB.create();
  const stellar = new Stellar(transport);

  // Get public key (using default path: 44'/148'/0')
  const result = await stellar.getPublicKey("44'/148'/0'");
  
  return {
    publicKey: result.publicKey,
    sign: async (txXdr: string) => {
      const signature = await stellar.signTransaction("44'/148'/0'", txXdr);
      return signature.signature;
    },
  };
}
```

## Contract Calls

### Create Escrow

```typescript
import { xdr, Operation, BASE_FEE } from '@stellar/stellar-sdk';

async function createEscrow(
  publicKey: string,
  recipient: string,
  amount: bigint,
  deadline: number
): Promise<string> {
  // Build transaction
  const account = await server.getAccount(publicKey);
  
  const transaction = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      contract.call(
        'create_escrow',
        ...[
          new Address(recipient).toScVal(),
          nativeToScVal(amount, { type: 'i128' }),
          nativeToScVal(deadline, { type: 'u64' }),
        ]
      )
    )
    .setTimeout(180)
    .build();

  // Simulate transaction
  const simulated = await server.simulateTransaction(transaction);
  
  if (SorobanRpc.Api.isSimulationError(simulated)) {
    throw new Error(`Simulation failed: ${simulated.error}`);
  }

  // Prepare and sign transaction
  const prepared = SorobanRpc.assembleTransaction(transaction, simulated).build();
  const signedXdr = await signTransaction(prepared.toXDR(), {
    networkPassphrase: Networks.TESTNET,
  });

  // Submit transaction
  const tx = TransactionBuilder.fromXDR(signedXdr, Networks.TESTNET);
  const result = await server.sendTransaction(tx);

  // Wait for confirmation
  if (result.status === 'ERROR') {
    throw new Error(`Transaction failed: ${result.errorResult}`);
  }

  return result.hash;
}
```

### Release Escrow

```typescript
async function releaseEscrow(
  publicKey: string,
  escrowId: string
): Promise<string> {
  const account = await server.getAccount(publicKey);
  
  const transaction = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      contract.call(
        'release_escrow',
        nativeToScVal(escrowId, { type: 'string' })
      )
    )
    .setTimeout(180)
    .build();

  // Simulate, sign, and submit (same as above)
  const simulated = await server.simulateTransaction(transaction);
  const prepared = SorobanRpc.assembleTransaction(transaction, simulated).build();
  const signedXdr = await signTransaction(prepared.toXDR(), {
    networkPassphrase: Networks.TESTNET,
  });

  const tx = TransactionBuilder.fromXDR(signedXdr, Networks.TESTNET);
  const result = await server.sendTransaction(tx);

  return result.hash;
}
```

### Query Escrow Details

```typescript
async function getEscrowDetails(escrowId: string): Promise<EscrowDetails> {
  const account = await server.getAccount(publicKey); // Any account for read-only
  
  const transaction = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      contract.call(
        'get_escrow',
        nativeToScVal(escrowId, { type: 'string' })
      )
    )
    .setTimeout(180)
    .build();

  const simulated = await server.simulateTransaction(transaction);
  
  if (SorobanRpc.Api.isSimulationSuccess(simulated)) {
    return scValToNative(simulated.result.retval);
  }
  
  throw new Error('Failed to fetch escrow details');
}
```

## Event Listening

### Listen for Escrow Events

```typescript
async function listenForEscrowEvents(
  startLedger: number,
  callback: (event: ContractEvent) => void
): Promise<void> {
  const eventStream = server.getEvents({
    startLedger,
    filters: [
      {
        type: 'contract',
        contractIds: [CONTRACT_ID],
      },
    ],
  });

  for await (const event of eventStream) {
    const parsedEvent = parseEscrowEvent(event);
    callback(parsedEvent);
  }
}

function parseEscrowEvent(event: any): ContractEvent {
  const topic = event.topic[0];
  
  switch (topic) {
    case 'escrow_created':
      return {
        type: 'created',
        escrowId: scValToNative(event.topic[1]),
        depositor: scValToNative(event.topic[2]),
        recipient: scValToNative(event.topic[3]),
        amount: scValToNative(event.value),
      };
    
    case 'escrow_released':
      return {
        type: 'released',
        escrowId: scValToNative(event.topic[1]),
        recipient: scValToNative(event.topic[2]),
      };
    
    case 'escrow_refunded':
      return {
        type: 'refunded',
        escrowId: scValToNative(event.topic[1]),
        depositor: scValToNative(event.topic[2]),
      };
    
    default:
      return { type: 'unknown', raw: event };
  }
}
```

### Real-time Event Monitoring

```typescript
// Monitor events from latest ledger
async function startEventMonitoring() {
  const latestLedger = await server.getLatestLedger();
  let currentLedger = latestLedger.sequence;

  setInterval(async () => {
    const events = await server.getEvents({
      startLedger: currentLedger,
      filters: [{ type: 'contract', contractIds: [CONTRACT_ID] }],
    });

    events.events.forEach((event) => {
      console.log('New event:', parseEscrowEvent(event));
    });

    currentLedger = await server.getLatestLedger().then((l) => l.sequence);
  }, 5000); // Poll every 5 seconds
}
```

## Error Handling

### Common Errors and Solutions

```typescript
async function safeContractCall<T>(
  operation: () => Promise<T>
): Promise<{ success: boolean; data?: T; error?: string }> {
  try {
    const data = await operation();
    return { success: true, data };
  } catch (error) {
    // Parse error
    const errorMessage = parseContractError(error);
    return { success: false, error: errorMessage };
  }
}

function parseContractError(error: any): string {
  const message = error?.message || String(error);

  // Contract-specific errors
  if (message.includes('EscrowNotFound')) {
    return 'Escrow does not exist';
  }
  if (message.includes('Unauthorized')) {
    return 'You are not authorized to perform this action';
  }
  if (message.includes('DeadlinePassed')) {
    return 'The escrow deadline has already passed';
  }
  if (message.includes('InsufficientBalance')) {
    return 'Insufficient balance to create escrow';
  }

  // Network errors
  if (message.includes('timeout')) {
    return 'Transaction timed out. Please try again.';
  }
  if (message.includes('rejected')) {
    return 'Transaction was rejected by your wallet';
  }

  return 'An unexpected error occurred';
}
```

### Retry Logic

```typescript
async function retryOperation<T>(
  operation: () => Promise<T>,
  maxRetries = 3
): Promise<T> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await operation();
    } catch (error) {
      if (i === maxRetries - 1) throw error;
      
      // Exponential backoff
      await new Promise((resolve) => setTimeout(resolve, 1000 * Math.pow(2, i)));
    }
  }
  throw new Error('Max retries exceeded');
}
```

## Complete Examples

### React Component Example

```typescript
import { useState, useEffect } from 'react';

function EscrowManager() {
  const [publicKey, setPublicKey] = useState<string>('');
  const [escrows, setEscrows] = useState<any[]>([]);

  // Connect wallet
  const handleConnect = async () => {
    try {
      const key = await connectWallet();
      setPublicKey(key);
    } catch (error) {
      console.error('Failed to connect:', error);
    }
  };

  // Create escrow
  const handleCreateEscrow = async (recipient: string, amount: bigint) => {
    try {
      const deadline = Math.floor(Date.now() / 1000) + 86400; // 24 hours
      const txHash = await createEscrow(publicKey, recipient, amount, deadline);
      console.log('Escrow created:', txHash);
    } catch (error) {
      const errorMsg = parseContractError(error);
      alert(errorMsg);
    }
  };

  // Listen for events
  useEffect(() => {
    if (!publicKey) return;

    const startListening = async () => {
      const latestLedger = await server.getLatestLedger();
      await listenForEscrowEvents(latestLedger.sequence, (event) => {
        console.log('Event received:', event);
        // Update UI based on event
      });
    };

    startListening();
  }, [publicKey]);

  return (
    <div>
      {!publicKey ? (
        <button onClick={handleConnect}>Connect Wallet</button>
      ) : (
        <div>
          <p>Connected: {publicKey}</p>
          <button onClick={() => handleCreateEscrow('GXXX...', BigInt(1000000))}>
            Create Escrow
          </button>
        </div>
      )}
    </div>
  );
}
```

## Testing on Devnet

### Prerequisites
```bash
# Install Stellar CLI
cargo install --locked stellar-cli

# Create test accounts
stellar keys generate alice --network testnet
stellar keys generate bob --network testnet

# Fund accounts
stellar keys fund alice --network testnet
stellar keys fund bob --network testnet
```

### Deploy Contract
```bash
# Build contract
cd contracts/escrow
stellar contract build

# Deploy to testnet
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source alice \
  --network testnet
```

### Test Integration
```typescript
// Run integration tests
const aliceKey = 'GXXX...'; // Alice's public key
const bobKey = 'GYYY...'; // Bob's public key

// Test escrow flow
const txHash = await createEscrow(aliceKey, bobKey, BigInt(1000000), deadline);
console.log('Created:', txHash);

// Wait for confirmation
await waitForConfirmation(txHash);

// Release escrow
const releaseTx = await releaseEscrow(aliceKey, escrowId);
console.log('Released:', releaseTx);
```

## Additional Resources
- [Stellar SDK Documentation](https://developers.stellar.org/docs)
- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Freighter Wallet](https://www.freighter.app/)
- [Contract Source Code](../contracts/escrow)

## Support
For issues or questions, please open an issue on the GitHub repository.
