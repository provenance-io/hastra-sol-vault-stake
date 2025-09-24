import * as anchor from "@coral-xyz/anchor";
import {Program} from "@coral-xyz/anchor";
import {HastraSolVaultStake} from "../target/types/hastra_sol_vault_stake";
import {PublicKey} from "@solana/web3.js";
import { createHash } from "crypto";
import { MerkleTree } from "merkletreejs";
import yargs from "yargs";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.HastraSolVaultStake as Program<HastraSolVaultStake>;

const args = yargs(process.argv.slice(2))
    .option("epoch", {
        type: "number",
        description: "Epoch index",
        required: true,
    })
    .option("reward_allocations", {
        type: "json",
        description: "Allocations object: {allocations: [{\"account\": \"3m7...sKf\", \"amount\": 1000}, ...]}",
        required: true,
    })
    .parseSync();

// helper: sha256 hash
const sha256 = (x: Buffer) => createHash("sha256").update(x).digest();

// leaf = sha256(user || amount_le_u64 || epochIndex_le_u64)
function makeLeaf(user: PublicKey, amount: anchor.BN | number, epoch: number): Buffer {
    return sha256(Buffer.concat([
        user.toBuffer(),
        (anchor.BN.isBN(amount) ? amount : new anchor.BN(amount)).toArrayLike(Buffer, "le", 8),
        new anchor.BN(epoch).toArrayLike(Buffer, "le", 8),
    ]));
}

const main = async () => {
    const epochIndex = args.epoch;
    const allocations: {user: PublicKey, amount: anchor.BN}[] = (JSON.parse(args.reward_allocations).allocations as {account: string, amount: number}[]).map((a: {account: string, amount: number}) => {
        return {user: new PublicKey(a.account), amount: new anchor.BN(a.amount)};
    });

    console.log("Epoch:", epochIndex.toString());
    console.log("Allocations:", allocations.map(a => ({user: a.user.toBase58(), amount: a.amount.toString()})));

    // Merkle tree setup
    // build tree
    const leaves = allocations.map(a => makeLeaf(a.user, a.amount, epochIndex));
    const tree = new MerkleTree(leaves, sha256, { sortPairs: true });
    const root = tree.getRoot(); // Buffer
    const total = allocations.reduce((acc, a) => acc.add(a.amount), new anchor.BN(0));

    const [configPda, bump] = anchor.web3.PublicKey.findProgramAddressSync(
        [Buffer.from("config")],
        program.programId
    );
    const [epochPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("epoch"), new anchor.BN(epochIndex).toArrayLike(Buffer, "le", 8)],
        program.programId
    );

    const tx = await program.methods
        .createRewardsEpoch(new anchor.BN(epochIndex), Array.from(root), total)
        .accountsStrict({
            config: configPda,
            admin: provider.wallet.publicKey,
            epoch: epochPda,
            systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();

    console.log("Transaction:", tx);
};

main().catch(console.error);
