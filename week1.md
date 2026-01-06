# Builder Track Weekly Report — Week 1

**Name:** Ali Jouahri  
**Week Ending:** 06-17-2025

## Courses Completed

- Started and CKB Academy course <b>
- Completed the first **two modules** of the CKB Academy:

[![Screenshot-from-2026-01-06-20-55-12.png](https://i.postimg.cc/Jz9db3Wm/Screenshot-from-2026-01-06-20-55-12.png)](https://postimg.cc/FYVZvLRn)

## Key Learnings

### Cells

- Cells are the basic units of CKb and all constitute the general state of CKB ,calls are like UTXO of bitcoin but unlike UTXOs ,Cells can store data
- Cells act like Box that can contain data and the Size of this Box is determined by the amount of tokens stored within it ,in Nervous CKB one byte of space cost 1 CKB token

### Lock Ssript and Type script

- The Lock Script of a cell serves as a an authentication feature of the the cell determining who can access that cell and manipulate it
- The Type script is an optional field for specifying the characteristics of the script ,this basically determines what can be donw with the cell.

### Transactions

- Transactions basically entail destroying some cells and creating cells
- The capacity of the cells created must not exceed the ones destroyed
- The cost of the transactions is difference in capacity between the cells destroyed and the cells created .since 1Byte is 1CKB hence for a transaction with 100 capcity cell as input and a an 80 Byte capacity cells as output the fees is 100-80 = 20 CKB
- input cells are entered as Out points containing the the Transaction hash when the cell was created and the the index of the cell for the array of of cells created at that transaction
- Witnesses serve as prove for authorised transaction

### NFTs

- There are two current NFT standards on the CKB Nervous Network
- **CoTA** behaves closely to the ERC721 standards on EVM chains in that the content of the MFT is stored off chain.each user is assigned one CoTA cell and the that cell contains a 32-byte root hash and this 32-byte proof is all that is needed to hold the proof data for an unlimited amount of NFTs
- **Spore** seeks a more decentralised ,tamper-proof approach where the data of the NFT is stored directly on the chain ,it's idea; for high value assets and low data assets like pixel art

## Practical Progress

### First Trnsaction\*\*

- I built a tranaction manually specifying the cellDeps,Input Cells as Outpoints and Output Cells and the Witneses
  [![buiding-Tx.png](https://i.postimg.cc/pTxRRhk9/buiding-Tx.png)](https://postimg.cc/hJCH0Gkg)
- I initiated my first transaction on the testnet [Find Details Here](https://testnet.explorer.nervos.org/transaction/0x7d96cadce61d61f49b2f1ece85adadac933de9a30ebb83830a6b7500ae11a019)
