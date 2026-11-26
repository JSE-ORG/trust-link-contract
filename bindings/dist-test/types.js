export var EscrowState;
(function (EscrowState) {
    EscrowState["Pending"] = "Pending";
    EscrowState["Funded"] = "Funded";
    EscrowState["Shipped"] = "Shipped";
    EscrowState["Completed"] = "Completed";
    EscrowState["Disputed"] = "Disputed";
    EscrowState["Refunded"] = "Refunded";
    EscrowState["Canceled"] = "Canceled";
})(EscrowState || (EscrowState = {}));
export var DisputeStatus;
(function (DisputeStatus) {
    DisputeStatus["Active"] = "Active";
    DisputeStatus["Resolved"] = "Resolved";
})(DisputeStatus || (DisputeStatus = {}));
export var ResolutionType;
(function (ResolutionType) {
    ResolutionType["Release"] = "Release";
    ResolutionType["Refund"] = "Refund";
})(ResolutionType || (ResolutionType = {}));
export var ContractError;
(function (ContractError) {
    ContractError[ContractError["InvalidAmount"] = 1] = "InvalidAmount";
    ContractError[ContractError["InsufficientBalance"] = 2] = "InsufficientBalance";
    ContractError[ContractError["EscrowNotFound"] = 3] = "EscrowNotFound";
    ContractError[ContractError["InvalidState"] = 4] = "InvalidState";
    ContractError[ContractError["NotAuthorized"] = 5] = "NotAuthorized";
    ContractError[ContractError["AlreadyInitialized"] = 6] = "AlreadyInitialized";
    ContractError[ContractError["FeeExceedsMax"] = 7] = "FeeExceedsMax";
    ContractError[ContractError["EscrowHasNoBuyer"] = 8] = "EscrowHasNoBuyer";
    ContractError[ContractError["ShippingWindowNotElapsed"] = 9] = "ShippingWindowNotElapsed";
    ContractError[ContractError["InvalidEvidenceHash"] = 10] = "InvalidEvidenceHash";
    ContractError[ContractError["DisputeNotFound"] = 11] = "DisputeNotFound";
    ContractError[ContractError["ContractPaused"] = 12] = "ContractPaused";
    ContractError[ContractError["InvalidTrackingId"] = 13] = "InvalidTrackingId";
    ContractError[ContractError["EscrowExpired"] = 25] = "EscrowExpired";
})(ContractError || (ContractError = {}));
