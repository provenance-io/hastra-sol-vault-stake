import * as anchor from "@coral-xyz/anchor";
import {Program} from "@coral-xyz/anchor";
import {HastraSolVaultStake} from "../target/types/hastra_sol_vault_stake";
import {PublicKey} from "@solana/web3.js";
import {createHash} from "crypto";
import {MerkleTree} from "merkletreejs";
import yargs from "yargs";
import {getAssociatedTokenAddressSync} from "@solana/spl-token";

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
        type: "string",
        description: "Allocations object: {allocations: [{\"account\": \"3m7...sKf\", \"amount\": 1000}, ...]}",
        required: true,
    })
    .option("mint", {
        type: "string",
        description: "Token that will be minted (e.g. PRIME) upon validation of the claim proof",
        required: true,
    })
    .option("amount", {
        type: "number",
        description: "Amount to claim from this epoch index",
        required: false,
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
    console.log("User:", provider.wallet.publicKey.toBase58());

    // Merkle tree setup
    // build tree
    const leaves = allocations.map(a => makeLeaf(a.user, a.amount, epochIndex));
    const tree = new MerkleTree(leaves, sha256, { sortPairs: true });

    const leaf = makeLeaf(provider.wallet.publicKey, args.amount ?? 0, epochIndex);
    const proof: number[][] = tree
        .getProof(leaf)
        .map(p => Array.from(p.data as Buffer));

    const [configPda, bump] = anchor.web3.PublicKey.findProgramAddressSync(
        [Buffer.from("config")],
        program.programId
    );
    const [epochPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("epoch"), new anchor.BN(epochIndex).toArrayLike(Buffer, "le", 8)],
        program.programId
    );
    // derive claim record PDA
    const [claimPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("claim"), epochPda.toBuffer(), provider.wallet.publicKey.toBuffer()],
        program.programId
    );

    const [mintAuthorityPda] = anchor.web3.PublicKey.findProgramAddressSync(
        [Buffer.from("mint_authority")],
        program.programId
    );

    const mint = new anchor.web3.PublicKey(args.mint);
    // Calculate the Associated Token Account address
    const tokenAccount = getAssociatedTokenAddressSync(
        mint,
        provider.wallet.publicKey,
    );

    const tx = await program.methods
        .claimRewards(new anchor.BN(args.amount), proof)
        .accountsStrict({
            config: configPda,
            user: provider.wallet.publicKey,
            epoch: epochPda,
            claimRecord: claimPda,
            mintAuthority: mintAuthorityPda,
            mint: mint,
            userStakeTokenAccount: tokenAccount,
            systemProgram: anchor.web3.SystemProgram.programId,
            tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID
        })
        .rpc();

    console.log("Transaction:", tx);
};

main().catch(console.error);
