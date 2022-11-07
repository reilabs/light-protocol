const light = require('../../light-protocol-sdk');
const {U64, I64} = require('n64');
const anchor = require("@project-serum/anchor")
const nacl = require('tweetnacl')
const FIELD_SIZE = new anchor.BN('21888242871839275222246405745257275088548364400416034343698204186575808495617');
export const createEncryptionKeypair = () => nacl.box.keyPair()
var assert = require('assert');
let circomlibjs = require("circomlibjs")
var ffjavascript = require('ffjavascript');
const { unstringifyBigInts, leInt2Buff } = ffjavascript.utils;

import {
  MERKLE_TREE_KEY,
  DEFAULT_PROGRAMS,
  ADMIN_AUTH_KEYPAIR,
  ADMIN_AUTH_KEY,
  MERKLE_TREE_SIZE,
  MERKLE_TREE_KP,
  MERKLE_TREE_SIGNER_AUTHORITY,
  PRIVATE_KEY,
  FIELD_SIZE,
  MINT_PRIVATE_KEY,
  MINT
  } from "./constants";
import {Connection, PublicKey, Keypair, SystemProgram, TransactionMessage, ComputeBudgetProgram,  AddressLookupTableAccount, VersionedTransaction, sendAndConfirmRawTransaction } from "@solana/web3.js";
import { newAccountWithLamports  } from "./test_transactions";
import { TOKEN_PROGRAM_ID, getAccount  } from '@solana/spl-token';
import {checkRentExemption} from './test_checks';
import {unpackLeavesAccount} from './unpack_accounts';

// add verifier class which is passed in with the constructor
// this class replaces the send transaction, also configures path the provingkey and witness, the inputs for the integrity hash
// input custom verifier with three functions by default prepare, proof, send
// include functions from sdk in shieldedTransaction
//
export class shieldedTransaction {
  constructor({
    keypair, // : Keypair shielded pool keypair that is derived from seedphrase. OutUtxo: supply pubkey
    encryptionKeypair = createEncryptionKeypair(),
    relayerFee = U64(10_000),
    merkleTreeIndex = 0,
    merkleTreePubkey,
    merkleTree = null,
    merkleTreeAssetPubkey = null,
    recipient, //PublicKey
    // recipientFee: number,
    lookupTable, //PublicKey
    payer, //: Keypair
    relayerPubkey = null, //PublicKey
    merkleTreeProgram, // any
    verifierProgram,//: any
    merkle_tree_token_pda,
    preInsertedLeavesIndex,
    provider,
    merkleTreeFeeAssetPubkey,
    relayerRecipient,
    registeredVerifierPda,
    poseidon,
    sendTransaction
  }) {
      if (relayerPubkey == null) {
          this.relayerPubkey = new PublicKey(payer.publicKey);
      } else {
         this.relayerPubkey = relayerPubkey;
      }
      this.relayerRecipient = relayerRecipient;
      this.preInsertedLeavesIndex = preInsertedLeavesIndex;
      this.merkleTreeProgram = merkleTreeProgram;
      this.verifierProgram = verifierProgram;
      this.lookupTable = lookupTable;
      this.feeAsset = new anchor.BN(anchor.web3.SystemProgram.programId._bn.toString()).mod(FIELD_SIZE);
      this.relayerFee = relayerFee;
      this.merkleTreeIndex = merkleTreeIndex;
      this.merkleTreePubkey = merkleTreePubkey;
      this.merkleTreeAssetPubkey = merkleTreeAssetPubkey;
      this.merkleTree = null;
      this.utxos = [];
      this.feeUtxos = [];
      this.encryptionKeypair = encryptionKeypair;
      this.payer = payer;

      this.provider = provider;
      this.recipient = recipient;
      this.merkleTreeFeeAssetPubkey = merkleTreeFeeAssetPubkey;
      this.keypair = keypair;
      this.registeredVerifierPda = registeredVerifierPda;
      this.merkleTree = merkleTree;
      this.poseidon = poseidon;
      this.sendTransaction = sendTransaction;
    }

    async getMerkleTree() {
      this.poseidon = await circomlibjs.buildPoseidonOpt();
      if (this.keypair == null) {
        this.keypair = new light.Keypair(this.poseidon);
      }
      this.merkleTree = await light.buildMerkelTree(this.poseidon, 18, []);
      this.merkleTreeLeavesIndex = 0;

    }

    async getRootIndex() {
      let root = Uint8Array.from(leInt2Buff(unstringifyBigInts(this.merkleTree.root()), 32));
      let merkle_tree_account = await this.provider.connection.getAccountInfo(this.merkleTreePubkey);
      let merkle_tree_account_data  = this.merkleTreeProgram.account.merkleTree._coder.accounts.decode('MerkleTree', merkle_tree_account.data);

       merkle_tree_account_data.roots.map((x, index)=> {
        if (x.toString() === root.toString()) {
          this.root_index =  index;
        }
      })

    }

    async prepareTransaction() {
      let data = await light.prepareTransaction(
       this.inputUtxos,
       this.outputUtxos,
       this.merkleTree,
       this.merkleTreeIndex,
       this.merkleTreePubkey.toBytes(),
       this.externalAmountBigNumber,
       this.relayerFee,
       this.recipient, // recipient
       this.relayerPubkey,
       this.action,
       this.encryptionKeypair,
       this.inIndices,
       this.outIndices,
       this.assetPubkeys,
       this.mintPubkey,
       false,
       this.feeAmount,
       this.recipientFee
     )
     this.input = data.input;
     this.extAmount = data.extAmount;
     this.externalAmountBigNumber = data.externalAmountBigNumber;
     this.extDataBytes = data.extDataBytes;
     this.encryptedOutputs = data.extDataBytes;
    }

    async prepareTransactionFull({
      inputUtxos,
      outputUtxos,
      action,
      assetPubkeys,
      recipient,
      mintPubkey = 0,
      relayerFee = null, // public amount of the fee utxo adjustable if you want to deposit a fee utxo alongside your spl deposit
      shuffle = true,
      recipientFee,
      sender
    }) {
      mintPubkey = assetPubkeys[1];
      if (assetPubkeys[0].toString() != this.feeAsset.toString()) {
        throw "feeAsset should be assetPubkeys[0]";
      }
      if (action == "DEPOSIT") {
        console.log("Deposit");

        this.relayerFee = relayerFee;
        this.sender = sender;
        this.senderFee  = new PublicKey(this.payer.publicKey);
        this.recipient = this.merkleTreeAssetPubkey;
        this.recipientFee = this.merkleTreeFeeAssetPubkey;

        if (this.relayerPubkey.toBase58() != new PublicKey(this.payer.publicKey).toBase58()) {
          throw "relayerPubkey and payer pubkey need to be equivalent at deposit";
        }
      } else if (action == "WITHDRAWAL") {
        this.senderFee = this.merkleTreeFeeAssetPubkey;
        this.recipientFee = recipientFee;
        this.sender = this.merkleTreeAssetPubkey;
        this.recipient = recipient;
        if (relayerFee != null) {
          this.relayerFee = relayerFee;
          if (relayerFee == undefined) {
            throw "relayerFee undefined";
          }
        }

      if (recipient == undefined) {
        throw "recipient undefined";
      }
      if (recipientFee == undefined) {
        throw "recipientFee undefined";
      }
    }

      this.assetPubkeys = assetPubkeys;
      this.mintPubkey = mintPubkey;
      this.action = action;

      let res = light.prepareUtxos(
          inputUtxos,
          outputUtxos,
          this.relayerFee,
          this.assetPubkeys,
          this.action,
          this.poseidon,
          shuffle
      );

      this.inputUtxos = res.inputUtxos;
      this.outputUtxos = res.outputUtxos;
      this.inIndices = res.inIndices;
      this.outIndices = res.outIndices;
      this.externalAmountBigNumber = res.externalAmountBigNumber;
      this.feeAmount = res.feeAmount;

      let data = await light.prepareTransaction(
       this.inputUtxos,
       this.outputUtxos,
       this.merkleTree,
       this.merkleTreeIndex,
       this.merkleTreePubkey.toBytes(),
       this.externalAmountBigNumber,
       this.relayerFee,
       this.recipient,
       this.relayerPubkey,
       this.action,
       this.encryptionKeypair,
       this.inIndices,
       this.outIndices,
       this.assetPubkeys,
       this.mintPubkey,
       false,
       this.feeAmount,
       this.recipientFee
     )
     this.input = data.input;
     assert(this.input.mintPubkey == this.mintPubkey);
     assert(this.input.mintPubkey == this.assetPubkeys[1]);
     this.extAmount = data.extAmount;
     this.externalAmountBigNumber = data.externalAmountBigNumber;
     this.extDataBytes = data.extDataBytes;
     this.encrypedUtxos = data.encryptedUtxos
     if (this.externalAmountBigNumber != 0) {
       if (assetPubkeys[1].toString() != mintPubkey.toString()) {
         throw "mintPubkey should be assetPubkeys[1]";
       }
     }
    }

    async proof() {
      if (this.merkleTree == null) {
        throw "merkle tree not built";
      }
      if (this.inIndices == null) {
        throw "transaction not prepared";
      }
      await this.getRootIndex();

      let proofData = await light.getProofMasp(
        this.input,
        this.extAmount,
        this.externalAmountBigNumber,
        this.extDataBytes,
        this.encrypedUtxos
      )

      this.proofData = proofData;
      await this.getPdaAddresses()
      return this.proofData;
    }

    async getPdaAddresses() {
      let tx_integrity_hash = this.proofData.publicInputs.txIntegrityHash;
      let nullifiers = this.proofData.publicInputs.nullifiers;
      let leftLeaves = [this.proofData.publicInputs.leaves[0]];
      let merkleTreeProgram = this.merkleTreeProgram;
      let verifierProgram = this.verifierProgram;
      let signer = this.payer.publicKey;

      let nullifierPdaPubkeys = [];
      for (var i in nullifiers) {
        nullifierPdaPubkeys.push(
        (await PublicKey.findProgramAddress(
            [Buffer.from(new Uint8Array(nullifiers[i])), anchor.utils.bytes.utf8.encode("nf")],
            merkleTreeProgram.programId))[0]);
      }

      let leavesPdaPubkeys = [];
      for (var i in leftLeaves) {
        leavesPdaPubkeys.push(
        (await PublicKey.findProgramAddress(
            [Buffer.from(Array.from(leftLeaves[i]).reverse()), anchor.utils.bytes.utf8.encode("leaves")],
            merkleTreeProgram.programId))[0]);
      }

      let pdas = {
        signerAuthorityPubkey: (await PublicKey.findProgramAddress(
            [merkleTreeProgram.programId.toBytes()],
            verifierProgram.programId))[0],

        escrow: (await PublicKey.findProgramAddress(
            [anchor.utils.bytes.utf8.encode("escrow")],
            verifierProgram.programId))[0],
        verifierStatePubkey: (await PublicKey.findProgramAddress(
            [signer.toBytes(), anchor.utils.bytes.utf8.encode("VERIFIER_STATE")],
            verifierProgram.programId))[0],
        feeEscrowStatePubkey: (await PublicKey.findProgramAddress(
            [Buffer.from(new Uint8Array(tx_integrity_hash)), anchor.utils.bytes.utf8.encode("escrow")],
            verifierProgram.programId))[0],
        merkleTreeUpdateState: (await PublicKey.findProgramAddress(
            [Buffer.from(new Uint8Array(leftLeaves[0])), anchor.utils.bytes.utf8.encode("storage")],
            merkleTreeProgram.programId))[0],
        nullifierPdaPubkeys,
        leavesPdaPubkeys,
        tokenAuthority: (await PublicKey.findProgramAddress(
            [anchor.utils.bytes.utf8.encode("spl")],
            merkleTreeProgram.programId
          ))[0],
      };
      this.escrow = pdas.escrow;
      this.leavesPdaPubkeys = pdas.leavesPdaPubkeys;
      this.nullifierPdaPubkeys = pdas.nullifierPdaPubkeys;
      this.signerAuthorityPubkey = pdas.signerAuthorityPubkey;
      this.tokenAuthority = pdas.tokenAuthority;
      this.verifierStatePubkey = pdas.verifierStatePubkey;
    }

    async checkBalances(){
      // Checking that nullifiers were inserted
      this.is_token = true;

      for (var i in this.nullifierPdaPubkeys) {

        var nullifierAccount = await this.provider.connection.getAccountInfo(
          this.nullifierPdaPubkeys[i],
          {
          commitment: 'confirmed',
          preflightCommitment: 'confirmed',
        }
        );

        await checkRentExemption({
          account: nullifierAccount,
          connection: this.provider.connection
        });
      }
      let leavesAccount
      var leavesAccountData
      // Checking that leaves were inserted
      for (var i in this.leavesPdaPubkeys) {

        leavesAccountData = await this.merkleTreeProgram.account.twoLeavesBytesPda.fetch(
          this.leavesPdaPubkeys[i]
        );

        try {

          assert(leavesAccountData.nodeLeft.toString() === this.proofData.publicInputs.leaves[0].reverse().toString(), "left leaf not inserted correctly")
          assert(leavesAccountData.nodeRight.toString() === this.proofData.publicInputs.leaves[1].reverse().toString(), "right leaf not inserted correctly")
          assert(leavesAccountData.merkleTreePubkey.toBase58() === this.merkleTreePubkey.toBase58(), "merkleTreePubkey not inserted correctly")
          for (var i in this.encrypedUtxos) {

            if (leavesAccountData.encryptedUtxos[i] !== this.encrypedUtxos[i]) {
              console.log(i);
            }
            assert(leavesAccountData.encryptedUtxos[i] === this.encrypedUtxos[i], "encryptedUtxos not inserted correctly");
          }

        } catch(e) {
          console.log("leaves: ", e);
        }
      }

      console.log(`mode ${this.action}, this.is_token ${this.is_token}`);

      try {
        console.log("this.preInsertedLeavesIndex ", this.preInsertedLeavesIndex);

        var preInsertedLeavesIndexAccount = await this.provider.connection.getAccountInfo(
          this.preInsertedLeavesIndex
        )

        console.log(preInsertedLeavesIndexAccount);
        const preInsertedLeavesIndexAccountAfterUpdate = this.merkleTreeProgram.account.preInsertedLeavesIndex._coder.accounts.decode('PreInsertedLeavesIndex', preInsertedLeavesIndexAccount.data);
        console.log("Number(preInsertedLeavesIndexAccountAfterUpdate.nextIndex) ", Number(preInsertedLeavesIndexAccountAfterUpdate.nextIndex));
        console.log(`${Number(leavesAccountData.leftLeafIndex) } + ${this.leavesPdaPubkeys.length * 2}`);

        assert(Number(preInsertedLeavesIndexAccountAfterUpdate.nextIndex) == Number(leavesAccountData.leftLeafIndex) + this.leavesPdaPubkeys.length * 2)

      } catch(e) {
        console.log("preInsertedLeavesIndex: ", e);

      }

      if (this.action == "DEPOSIT" && this.is_token == false) {
        var recipientAccount = await this.provider.connection.getAccountInfo(this.recipient)
        assert(recipientAccount.lamports == (I64(this.recipientBalancePriorTx).add(this.proofData.externalAmountBigNumber.toString())).toString(), "amount not transferred correctly");

      } else if (this.action == "DEPOSIT" && this.is_token == true) {
        console.log("DEPOSIT and token");
        console.log("this.recipient: ", this.recipient);

          var recipientAccount = await getAccount(
          this.provider.connection,
          this.recipient,
          TOKEN_PROGRAM_ID
        );
        var recipientFeeAccountBalance = await this.provider.connection.getBalance(this.recipientFee);

        // console.log(`Balance now ${senderAccount.amount} balance beginning ${senderAccountBalancePriorLastTx}`)
        // assert(senderAccount.lamports == (I64(senderAccountBalancePriorLastTx) - I64.readLE(this.proofData.extAmount, 0)).toString(), "amount not transferred correctly");

        console.log(`Balance now ${recipientAccount.amount} balance beginning ${this.recipientBalancePriorTx}`)
        console.log(`Balance now ${recipientAccount.amount} balance beginning ${(Number(this.recipientBalancePriorTx) + Number(this.proofData.externalAmountBigNumber))}`)
        assert(recipientAccount.amount == (Number(this.recipientBalancePriorTx) + Number(this.proofData.externalAmountBigNumber)).toString(), "amount not transferred correctly");
        console.log(`Blanace now ${recipientFeeAccountBalance} ${Number(this.recipientFeeBalancePriorTx) + Number(this.feeAmount)}`);
        console.log("fee amount: ", this.feeAmount);
        console.log("fee amount from inputs. ", new anchor.BN(this.proofData.publicInputs.feeAmount.slice(24,32)).toString());
        console.log("pub amount from inputs. ", new anchor.BN(this.proofData.publicInputs.publicAmount.slice(24,32)).toString());

        console.log("recipientFeeBalancePriorTx: ", this.recipientFeeBalancePriorTx);

        var senderFeeAccountBalance = await this.provider.connection.getBalance(this.senderFee);
        console.log("senderFeeAccountBalance: ", senderFeeAccountBalance);
        console.log("this.senderFeeBalancePriorTx: ", this.senderFeeBalancePriorTx);

        assert(recipientFeeAccountBalance == Number(this.recipientFeeBalancePriorTx) + Number(this.feeAmount));
        console.log(`${Number(this.senderFeeBalancePriorTx)} - ${Number(this.feeAmount)} == ${senderFeeAccountBalance}`);
        assert(Number(this.senderFeeBalancePriorTx) - Number(this.feeAmount) - 5000 == Number(senderFeeAccountBalance) );

      } else if (this.action == "WITHDRAWAL" && this.is_token == false) {
        var senderAccount = await this.provider.connection.getAccountInfo(this.sender)
        var recipientAccount = await this.provider.connection.getAccountInfo(this.recipient)
        // console.log("senderAccount.lamports: ", senderAccount.lamports)
        // console.log("I64(senderAccountBalancePriorLastTx): ", I64(senderAccountBalancePriorLastTx).toString())
        // console.log("Sum: ", ((I64(senderAccountBalancePriorLastTx).add(I64.readLE(this.proofData.extAmount, 0))).sub(I64(relayerFee))).toString())

        assert(senderAccount.lamports == ((I64(senderAccountBalancePriorLastTx).add(I64.readLE(this.proofData.extAmount, 0))).sub(I64(relayerFee))).toString(), "amount not transferred correctly");

        var recipientAccount = await this.provider.connection.getAccountInfo(recipient)
        // console.log(`recipientAccount.lamports: ${recipientAccount.lamports} == sum ${((I64(Number(this.recipientBalancePriorTx)).sub(I64.readLE(this.proofData.extAmount, 0))).add(I64(relayerFee))).toString()}

        assert(recipientAccount.lamports == ((I64(Number(this.recipientBalancePriorTx)).sub(I64.readLE(this.proofData.extAmount, 0)))).toString(), "amount not transferred correctly");


      }  else if (this.action == "WITHDRAWAL" && this.is_token == true) {
        var senderAccount = await getAccount(
          this.provider.connection,
          this.sender,
          TOKEN_PROGRAM_ID
        );
        var recipientAccount = await getAccount(
          this.provider.connection,
          this.recipient,
          TOKEN_PROGRAM_ID
        );


        // assert(senderAccount.amount == ((I64(Number(senderAccountBalancePriorLastTx)).add(I64.readLE(this.proofData.extAmount, 0))).sub(I64(relayerFee))).toString(), "amount not transferred correctly");
        console.log(`${recipientAccount.amount}, ${((new anchor.BN(this.recipientBalancePriorTx)).sub(this.proofData.externalAmountBigNumber)).toString()}`)
        assert(recipientAccount.amount.toString() == ((new anchor.BN(this.recipientBalancePriorTx)).sub(this.proofData.externalAmountBigNumber)).toString(), "amount not transferred correctly");



        var relayerAccount = await this.provider.connection.getBalance(this.relayerRecipient);
        var recipientFeeAccount = await this.provider.connection.getBalance(this.recipientFee);
        console.log("recipientFeeAccount ", recipientFeeAccount);
        console.log("this.feeAmount: ", this.feeAmount);
        console.log("recipientFeeBalancePriorTx ", this.recipientFeeBalancePriorTx);
        console.log(`recipientFeeAccount ${(new anchor.BN(recipientFeeAccount).add(new anchor.BN(this.relayerFee.toString()))).add(new anchor.BN("5000")).toString()} == ${new anchor.BN(this.recipientFeeBalancePriorTx).sub(new anchor.BN(this.feeAmount)).toString()}`)

        console.log("relayerAccount ", relayerAccount);
        console.log("this.relayerFee: ", this.relayerFee);
        console.log("relayerRecipientAccountBalancePriorLastTx ", this.relayerRecipientAccountBalancePriorLastTx);
        console.log(`relayerFeeAccount ${new anchor.BN(relayerAccount).sub(new anchor.BN(this.relayerFee.toString())).toString()} == ${new anchor.BN(this.relayerRecipientAccountBalancePriorLastTx)}`)

        // console.log(`relayerAccount ${new anchor.BN(relayerAccount).toString()} == ${new anchor.BN(this.relayerRecipientAccountBalancePriorLastTx).sub(new anchor.BN(this.relayerFee)).toString()}`)
        assert((new anchor.BN(recipientFeeAccount).add(new anchor.BN(this.relayerFee.toString()))).toString() == new anchor.BN(this.recipientFeeBalancePriorTx).sub(new anchor.BN(this.feeAmount)).toString());
        assert(new anchor.BN(relayerAccount).sub(new anchor.BN(this.relayerFee.toString())).add(new anchor.BN("5000")).toString() == new anchor.BN(this.relayerRecipientAccountBalancePriorLastTx).toString());



      } else {
        throw Error("mode not supplied");
      }
    }
}

export  async function sendTransaction(insert = true){

    try {
      this.recipientBalancePriorTx = (await getAccount(
        this.provider.connection,
        this.recipient,
        TOKEN_PROGRAM_ID
      )).amount;
    } catch(e) {
        // covers the case of the recipient being a native sol address not a spl token address
        try {
          this.recipientBalancePriorTx = await this.provider.connection.getBalance(this.recipient);
        } catch(e) {

        }
    }
    this.recipientFeeBalancePriorTx = await this.provider.connection.getBalance(this.recipientFee);
    // console.log("recipientBalancePriorTx: ", this.recipientBalancePriorTx);
    // console.log("recipientFeeBalancePriorTx: ", this.recipientFeeBalancePriorTx);
    // console.log("sender_fee: ", this.senderFee);
    this.senderFeeBalancePriorTx = await this.provider.connection.getBalance(this.senderFee);
    this.relayerRecipientAccountBalancePriorLastTx = await this.provider.connection.getBalance(this.relayerRecipient);

    // console.log("signingAddress:     ", this.relayerPubkey)
    // console.log("systemProgram:      ", SystemProgram.programId)
    // console.log("programMerkleTree:  ", this.merkleTreeProgram.programId)
    // console.log("rent:               ", DEFAULT_PROGRAMS.rent)
    // console.log("merkleTree:         ", this.merkleTreePubkey)
    // console.log("preInsertedLeavesInd", this.preInsertedLeavesIndex)
    // console.log("authority:          ", this.signerAuthorityPubkey)
    // console.log("tokenProgram:       ", TOKEN_PROGRAM_ID)
    // console.log("sender:             ", this.sender)
    // console.log("recipient:          ", this.recipient)
    // console.log("senderFee:          ", this.senderFee)
    // console.log("recipientFee:       ", this.recipientFee)
    // console.log("relayerRecipient:   ", this.relayerRecipient)
    // console.log("escrow:             ", this.escrow)
    // console.log("tokenAuthority:     ", this.tokenAuthority)
    // console.log("registeredVerifierPd",this.registeredVerifierPda)
    // console.log("encryptedOutputs len ", this.proofData.encryptedOutputs.length);
    // console.log("this.proofData.encryptedOutputs[0], ", this.proofData.encryptedOutputs);

    const ix = await this.verifierProgram.methods.shieldedTransferInputs(
      Buffer.from(this.proofData.proofBytes),
      Buffer.from(this.proofData.publicInputs.publicAmount),
      this.proofData.publicInputs.nullifiers,
      this.proofData.publicInputs.leaves,
      Buffer.from(this.proofData.publicInputs.feeAmount),
      new anchor.BN(this.root_index.toString()),
      new anchor.BN(this.relayerFee.toString()),
      Buffer.from(this.proofData.encryptedOutputs.slice(0,174)) // remaining bytes can be used once tx sizes increase
    ).accounts(
      {
        signingAddress:     this.relayerPubkey,
        systemProgram:      SystemProgram.programId,
        programMerkleTree:  this.merkleTreeProgram.programId,
        rent:               DEFAULT_PROGRAMS.rent,
        merkleTree:         this.merkleTreePubkey,
        preInsertedLeavesIndex: this.preInsertedLeavesIndex,
        authority:          this.signerAuthorityPubkey,
        tokenProgram:       TOKEN_PROGRAM_ID,
        sender:             this.sender,
        recipient:          this.recipient,
        senderFee:          this.senderFee,
        recipientFee:       this.recipientFee,
        relayerRecipient:   this.relayerRecipient,
        escrow:             this.escrow,
        tokenAuthority:     this.tokenAuthority,
        registeredVerifierPda: this.registeredVerifierPda
      }
    )
    .remainingAccounts([
      { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[0]},
      { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[1]},
      { isSigner: false, isWritable: true, pubkey: this.leavesPdaPubkeys[0]}
    ])
    .signers([this.payer]).instruction()
    console.log("this.payer: ", this.payer);

    let recentBlockhash = (await this.provider.connection.getRecentBlockhash(("finalized"))).blockhash;
    let txMsg = new TransactionMessage({
          payerKey: this.payer.publicKey,
          instructions: [
            ComputeBudgetProgram.setComputeUnitLimit({units:1_400_000}),
            ix
          ],
          recentBlockhash: recentBlockhash})

    let lookupTableAccount = await this.provider.connection.getAccountInfo(this.lookupTable, "confirmed");

    let unpackedLookupTableAccount = AddressLookupTableAccount.deserialize(lookupTableAccount.data);

    let compiledTx = txMsg.compileToV0Message([{state: unpackedLookupTableAccount}]);
    compiledTx.addressTableLookups[0].accountKey = this.lookupTable

    let transaction = new VersionedTransaction(compiledTx);
    let retries = 3;
    let res
    while (retries > 0) {
      transaction.sign([this.payer])
      recentBlockhash = (await this.provider.connection.getRecentBlockhash(("finalized"))).blockhash;
      transaction.message.recentBlockhash = recentBlockhash;
      let serializedTx = transaction.serialize();

      try {
        console.log("serializedTx: ");

        res = await sendAndConfirmRawTransaction(this.provider.connection, serializedTx,
          {
            commitment: 'finalized',
            preflightCommitment: 'finalized',
          }
        );
        retries = 0;

      } catch (e) {
        retries--;
        if (retries == 0 || e.logs != undefined) {
          console.log(e);
          return e;
        }
      }

    }

    // storing utxos
    this.outputUtxos.map((utxo) => {
      if (utxo.amounts[1] != 0 && utxo.assets[1] != this.feeAsset) {
          this.utxos.push(utxo)
      }
      if (utxo.amounts[0] != 0 && utxo.assets[0].toString() == this.feeAsset.toString()) {
        this.feeUtxos.push(utxo)
      }
    })
    this.inIndices = null;
    // inserting output utxos into merkle tree
    if (insert != "NOINSERT") {
      for (var i = 0; i<this.outputUtxos.length; i++) {
        this.merkleTree.update(this.merkleTreeLeavesIndex, this.outputUtxos[i].getCommitment())
        this.merkleTreeLeavesIndex++;
      }
    }

    return res;
  }

export async function transferFirst(this) {
  console.log("in transferFirst");

  const ix1 = await this.verifierProgram.methods.shieldedTransferFirst(
    Buffer.from(this.proofData.publicInputs.publicAmount),
    this.proofData.publicInputs.nullifiers,
    this.proofData.publicInputs.leaves,
    Buffer.from(this.proofData.publicInputs.feeAmount),
    new anchor.BN(this.root_index.toString()),
    new anchor.BN(this.relayerFee.toString()),
    Buffer.from(this.proofData.encryptedOutputs)
  ).accounts(
    {
      signingAddress:     this.relayerPubkey,
      systemProgram:      SystemProgram.programId,
      verifierState:      this.verifierStatePubkey
    }
  )
  .signers([this.payer])
  .rpc({
    commitment: 'finalized',
    preflightCommitment: 'finalized',
  });
  console.log("ix1 success ", ix1);
}

export async function transferSecond(this) {
  const ix = await this.verifierProgram.methods.shieldedTransferSecond(
    Buffer.from(this.proofData.proofBytes)
  ).accounts(
    {
      signingAddress:     this.relayerPubkey,
      verifierState:      this.verifierStatePubkey,
      systemProgram:      SystemProgram.programId,
      programMerkleTree:  this.merkleTreeProgram.programId,
      rent:               DEFAULT_PROGRAMS.rent,
      merkleTree:         this.merkleTreePubkey,
      preInsertedLeavesIndex: this.preInsertedLeavesIndex,
      authority:          this.signerAuthorityPubkey,
      tokenProgram:       TOKEN_PROGRAM_ID,
      sender:             this.sender,
      recipient:          this.recipient,
      senderFee:          this.senderFee,
      recipientFee:       this.recipientFee,
      relayerRecipient:   this.relayerRecipient,
      escrow:             this.escrow,
      tokenAuthority:     this.tokenAuthority,
      registeredVerifierPda: this.registeredVerifierPda
    }
  )
  .remainingAccounts([
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[0]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[1]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[2]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[3]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[4]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[5]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[6]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[7]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[8]},
    { isSigner: false, isWritable: true, pubkey: this.nullifierPdaPubkeys[9]},
    { isSigner: false, isWritable: true, pubkey: this.leavesPdaPubkeys[0]}
  ])
  .signers([this.payer]).instruction();
  let recentBlockhash = (await this.provider.connection.getRecentBlockhash("finalized")).blockhash;


  let txMsg = new TransactionMessage({
        payerKey: this.payer.publicKey,
        instructions: [
          ComputeBudgetProgram.setComputeUnitLimit({units:1_400_000}),
          ix
        ],
        recentBlockhash: recentBlockhash})

  let lookupTableAccount = await this.provider.connection.getAccountInfo(this.lookupTable, "confirmed");

  let unpackedLookupTableAccount = AddressLookupTableAccount.deserialize(lookupTableAccount.data);

  let compiledTx = txMsg.compileToV0Message([{state: unpackedLookupTableAccount}]);
  compiledTx.addressTableLookups[0].accountKey = this.lookupTable

  let transaction = new VersionedTransaction(compiledTx);
  let retries = 3;
  let res
  while (retries > 0) {
    transaction.sign([this.payer])
    recentBlockhash = (await this.provider.connection.getRecentBlockhash("finalized")).blockhash;
    transaction.message.recentBlockhash = recentBlockhash;
    let serializedTx = transaction.serialize();

    try {
      console.log("serializedTx: ");

      res = await sendAndConfirmRawTransaction(this.provider.connection, serializedTx,
        {
          commitment: 'finalized',
          preflightCommitment: 'finalized',
        }
      );
      retries = 0;

    } catch (e) {
      console.log(e);
      retries--;
      if (retries == 0 || e.logs != undefined) {
        const ixClose = await this.verifierProgram.methods.closeVerifierState(
        ).accounts(
          {
            signingAddress:     this.relayerPubkey,
            verifierState:      this.verifierStatePubkey
          }
        )
        .signers([this.payer]).rpc({
                commitment: 'finalized',
                preflightCommitment: 'finalized',
              });
        return e;
      }
    }

  }
}

export async function sendTransaction10(insert = true){
  assert(this.nullifierPdaPubkeys.length == 10);
  let balance = await this.provider.connection.getBalance(this.signerAuthorityPubkey, {preflightCommitment: "confirmed", commitment: "confirmed"});
  if (balance === 0) {
    await this.provider.connection.confirmTransaction(await this.provider.connection.requestAirdrop(this.signerAuthorityPubkey, 1_000_000_000), {preflightCommitment: "confirmed", commitment: "confirmed"})
  }
  try {
    this.recipientBalancePriorTx = (await getAccount(
      this.provider.connection,
      this.recipient,
      TOKEN_PROGRAM_ID
    )).amount;

  } catch (error) {

  }
  this.recipientFeeBalancePriorTx = await this.provider.connection.getBalance(this.recipientFee);
  // console.log("recipientBalancePriorTx: ", this.recipientBalancePriorTx);
  // console.log("recipientFeeBalancePriorTx: ", this.recipientFeeBalancePriorTx);
  // console.log("sender_fee: ", this.senderFee);
  this.senderFeeBalancePriorTx = await this.provider.connection.getBalance(this.senderFee);
  this.relayerRecipientAccountBalancePriorLastTx = await this.provider.connection.getBalance(this.relayerRecipient);

  // console.log("signingAddress:     ", this.relayerPubkey)
  // console.log("systemProgram:      ", SystemProgram.programId)
  // console.log("programMerkleTree:  ", this.merkleTreeProgram.programId)
  // console.log("rent:               ", DEFAULT_PROGRAMS.rent)
  // console.log("merkleTree:         ", this.merkleTreePubkey)
  // console.log("preInsertedLeavesInd", this.preInsertedLeavesIndex)
  // console.log("authority:          ", this.signerAuthorityPubkey)
  // console.log("tokenProgram:       ", TOKEN_PROGRAM_ID)
  // console.log("sender:             ", this.sender)
  // console.log("recipient:          ", this.recipient)
  // console.log("senderFee:          ", this.senderFee)
  // console.log("recipientFee:       ", this.recipientFee)
  // console.log("relayerRecipient:   ", this.relayerRecipient)
  // console.log("escrow:             ", this.escrow)
  // console.log("tokenAuthority:     ", this.tokenAuthority)
  // console.log("registeredVerifierPd",this.registeredVerifierPda)
  // console.log("encryptedOutputs len ", this.proofData.encryptedOutputs.length);
  // console.log("this.proofData.encryptedOutputs[0], ", this.proofData.encryptedOutputs);
  console.log("this.verifierStatePubkey, ", this.verifierStatePubkey.toBase58());
  // console.log("this.proofData.publicInputs.nullifiers, ", this.proofData.publicInputs.nullifiers);
  // console.log("this.root_index ", this.root_index);
  // console.log("this.relayerFee ", this.relayerFee);
  // console.log("this.encryptedOutputs ", this.proofData.encryptedOutputs);
  this.transferFirst = transferFirst;
  this.transferSecond = transferSecond;

  let res = await this.transferFirst();
  res = await this.transferSecond();

  return res;
}