This document is structured as a **System Architecture & Logic Specification**. It removes marketing language and focuses on state machines, data structures, and economic logic.

You can copy and paste this entire block directly into an AI IDE (like Cursor, Replit, or a sophisticated LLM session) to begin the coding process.

---

# Project Specification: CommonGround (Lossless SocialFi Protocol)

**Target Network:** Nervos Network (Layer 1 CKB + Layer 2 Godwoken/Axon)
**Core Logic:** `ve(3,3)` Governance + Lossless Savings (Prize-Linked)
**Economic Split:** 30% Creator / 30% Voters / 30% Protocol / 10% Lottery

---

## 1. High-Level Architecture

The system operates as a hybrid application.

* **Layer 1 (Trust & Vault):** Holds the raw CKB assets in the Nervos DAO.
* **Layer 2 (Logic & Interaction):** Handles user deposits, mints `$SOCIAL` tokens, manages the 10% liquidity buffer, and executes the ve(3,3) voting.

### 1.1 Component Interaction

```mermaid
[User] --(Deposit CKB)--> [L2 Buffer Contract]
                             |
                             |--[10% Liquid]--> [L2 Reserve Pool]
                             |
                             |--[90% Locked]--> [L1 Bridge] --(Deposit)--> [Nervos DAO]

```

---

## 2. Smart Contract Data Structures (Pseudo-Code)

These definitions govern the L2 Application Logic.

### 2.1 Core State Variables

```solidity
// Global Protocol State
struct ProtocolState {
    uint256 totalTVL;          // Total CKB deposited in protocol
    uint256 bufferBalance;     // CKB currently sitting in L2 contract (Liquid)
    uint256 daoLockedBalance;  // CKB currently locked in L1 Nervos DAO
    uint256 currentEpoch;      // Weekly epoch counter
    uint256 rewardRate;        // $SOCIAL tokens emitted per second
}

// Creator Pool Data
struct CreatorPool {
    address creatorAddress;
    uint256 poolTVL;           // Total CKB deposited for this creator
    uint256 accSocialPerShare; // For farming calculations
    uint256 totalVotes;        // veSOCIAL votes received this epoch
    uint256 bribeBalance;      // Extra rewards added by creator/sponsors
}

// User Deposit Receipt
struct DepositTicket {
    uint256 amount;            // Amount of CKB deposited
    uint256 timestamp;         // When deposit happened
    uint256 lockEnd;           // 0 if liquid, timestamp if in Queue
    bool inWithdrawalQueue;    // If true, waiting for L1 unlock
}

```

---

## 3. The "Buffer" Liquidity Mechanism

**Objective:** Maintain instant withdrawals for 95% of use cases while maximizing yield in L1 DAO.

### 3.1 Constants & Thresholds

* `TARGET_BUFFER_RATIO`: 10% (1000 basis points)
* `MIN_BUFFER_RATIO`: 5%
* `MAX_BUFFER_RATIO`: 15%
* `DAO_UNLOCK_CYCLE`: ~30 Days (180 Epochs on Nervos)

### 3.2 Deposit Logic (State Transition)

**Input:** User deposits `x` CKB.

1. **Mint:** Protocol mints `stCKB` (or internal accounting equivalent) to User.
2. **Route:**
* If `bufferBalance` < `TARGET_BUFFER_RATIO`: Keep `x` in L2 Buffer.
* Else: Batch `x` for bridging to L1 Nervos DAO.



### 3.3 Withdrawal Logic (The Critical Path)

**Input:** User requests withdrawal of `y` CKB.

```python
def execute_withdraw(user, amount):
    if bufferBalance >= amount:
        # FAST PATH (Instant)
        bufferBalance -= amount
        transfer(user, amount)
        burn_receipt(user, amount)
    else:
        # SLOW PATH (Queue System)
        remaining_liquid = bufferBalance
        shortfall = amount - remaining_liquid
        
        # 1. Give user whatever is liquid
        transfer(user, remaining_liquid)
        bufferBalance = 0
        
        # 2. Issue "QueueTicket" for the rest
        issue_queue_ticket(user, shortfall)
        
        # 3. Trigger L1 Unlock
        trigger_dao_withdrawal(shortfall) # Takes 30 days

```

---

## 4. The Economic Engine (Yield & Emissions)

### 4.1 The "30/30/30/10" Yield Router

This logic executes whenever Real Yield (CKB) is bridged back from the Nervos DAO (monthly).

* `Yield_Generated` = `Total_CKB_From_DAO` - `Principal`.

```solidity
function distributeYield(uint256 yieldAmount) {
    uint256 creatorShare = yieldAmount * 30 / 100;
    uint256 voterShare   = yieldAmount * 30 / 100;
    uint256 protocolFee  = yieldAmount * 30 / 100;
    uint256 lotteryPool  = yieldAmount * 10 / 100;

    // A. Pay Creator
    sendTo(creatorAddress, creatorShare);

    // B. Pay Voters (Distribute to Bribe Contract)
    distributeToVoters(voterShare);

    // C. Protocol Treasury
    sendTo(treasury, protocolFee);

    // D. Lottery Execution
    address winner = verifyRandomnessAndPick(allDepositors);
    sendTo(winner, lotteryPool);
}

```

### 4.2 The ve(3,3) Emission Logic

This governs the `$SOCIAL` token inflation.

* **Weekly Cycle:** Every 7 days, `emissions` are directed to pools.
* **Weight Calculation:**
`Pool_Weight` = `Votes_For_Pool` / `Total_Votes_Cast`
* **Reward:**
`Pool_Reward` = `Total_Weekly_Emission` * `Pool_Weight`
* **User Incentive:** Users holding CKB in a pool with a high `Pool_Reward` earn more `$SOCIAL` per second.

---

## 5. Development Roadmap (Phased Execution)

### Phase 1: The Vault (MVP)

* **Goal:** Allow deposits, manage the Buffer, and integrate Nervos DAO.
* **Requirement:** Build the "Buffer Manager" contract.
* **Restriction:** No `$SOCIAL` token yet. Yield is 100% to Depositor for simplicity to test the L1<->L2 bridge.

### Phase 2: The Social Token

* **Goal:** Launch `$SOCIAL` farming.
* **Requirement:** Deploy `MasterChef` style contract (standard yield farming logic).
* **Metric:** Users deposit CKB -> Earn `$SOCIAL`.

### Phase 3: The Governance (ve3,3)

* **Goal:** Launch Voting and the 30/30/30/10 split.
* **Requirement:**
* Deploy `veSOCIAL` (Voting Escrow) contract.
* Deploy `Bribe` contract.
* Activate the Yield Router logic.



---

## 6. Prompt for AI Agent

*Use this prompt to start the coding session:*

> "Act as a Senior Blockchain Developer. We are building 'CommonGround,' a SocialFi application on Nervos Network (Godwoken L2).
> **Core Task:** Create the Solidity smart contract for the **'BufferManager'**.
> **Constraints:**
> 1. The contract accepts CKB deposits.
> 2. It maintains a target liquidity ratio of 10%.
> 3. It has a function `routeFunds()`: If liquidity > 10%, it emits an event to bridge funds to L1 DAO.
> 4. It has a function `withdraw()`: It checks available liquidity. If insufficient, it reverts with a custom error `InsufficientLiquidBuffer()` (we will handle the queue logic in the next step).
> 5. Use OpenZeppelin Ownable and ReentrancyGuard.
> 
> 
> Please write the Solidity code for this component."