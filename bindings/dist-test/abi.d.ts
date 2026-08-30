export declare const contractName = "Escrow";
export declare const contractAbi: {
    readonly contractName: "Escrow";
    readonly functions: readonly [{
        readonly name: "initialize";
        readonly inputs: readonly ["admin", "fee_collector"];
        readonly output: "void";
    }, {
        readonly name: "pause_contract";
        readonly inputs: readonly [];
        readonly output: "void";
    }, {
        readonly name: "unpause_contract";
        readonly inputs: readonly [];
        readonly output: "void";
    }, {
        readonly name: "withdraw_fe  es";
        readonly inputs: readonly ["token", "to", "amount"];
        readonly output: "void";
    }, {
        readonly name: "create_escrow";
        readonly inputs: readonly ["seller", "resolver", "token", "amount", "fee_bps", "shipping_window"];
        readonly output: "u64";
    }, {
        readonly name: "fund_escrow";
        readonly inputs: readonly ["escrow_id", "buyer"];
        readonly output: "void";
    }, {
        readonly name: "mark_shipped";
        readonly inputs: readonly ["caller", "escrow_id", "tracking_id"];
        readonly output: "void";
    }, {
        readonly name: "confirm_delivery";
        readonly inputs: readonly ["caller", "escrow_id"];
        readonly output: "void";
    }, {
        readonly name: "raise_dispute";
        readonly inputs: readonly ["escrow_id", "reason", "description", "evidence_hash"];
        readonly output: "void";
    }, {
        readonly name: "resolve_dispute";
        readonly inputs: readonly ["escrow_id", "resolution"];
        readonly output: "void";
    }, {
        readonly name: "auto_release";
        readonly inputs: readonly ["escrow_id"];
        readonly output: "void";
    }, {
        readonly name: "get_escrow";
        readonly inputs: readonly ["escrow_id"];
        readonly output: "EscrowData";
    }, {
        readonly name: "get_dispute";
        readonly inputs: readonly ["escrow_id"];
        readonly output: "DisputeData";
    }, {
        readonly name: "get_fee_config";
        readonly inputs: readonly [];
        readonly output: "FeeConfig";
    }, {
        readonly name: "set_arbitration_fee";
        readonly inputs: readonly ["caller", "fee_bps"];
        readonly output: "void";
    }, {
        readonly name: "get_arbitration_fee";
        readonly inputs: readonly [];
        readonly output: "u32";
    }];
    readonly types: readonly ["EscrowState", "DisputeStatus", "ResolutionType", "ContractError", "FeeConfig", "FeesWithdrawn", "ContractPausedEvent", "ContractUnpausedEvent", "EscrowData", "DisputeData"];
};
//# sourceMappingURL=abi.d.ts.map