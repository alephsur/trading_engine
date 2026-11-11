use anyhow::{Result, ensure};
use std::io::{IsTerminal, Write};
use trading_advisor::{Approval, Context, Evidence, ImportedAdvice, Proposal};
use trading_backtest::{Config, data::Dataset, hash};
fn read<T: serde::de::DeserializeOwned>(path: &str, max: u64) -> Result<T> {
    ensure!(std::fs::metadata(path)?.len() <= max, "input too large");
    Ok(serde_json::from_reader(std::io::BufReader::new(
        std::fs::File::open(path)?,
    ))?)
}
fn write(path: &str, value: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(&bytes)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}
fn evidence(proposal: &str, data: &str, plan: &str, minimum: &str) -> Result<(Proposal, Evidence)> {
    let p: Proposal = read(proposal, 1_000_000)?;
    let d: Dataset = read(data, 512 * 1024 * 1024)?;
    let c: Config = read(plan, 1_000_000)?;
    let e = trading_advisor::evaluate(&p, &d, &c, minimum.parse()?)?;
    Ok((p, e))
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, context, out] if cmd == "prompt" => {
            let c: Context = read(context, 200_000)?;
            write(out, &trading_advisor::prompt(&c)?)?;
        }
        [cmd, context, advice, out] if cmd == "propose" => {
            let c = read(context, 200_000)?;
            let a = ImportedAdvice(read(advice, 200_000)?);
            write(out, &trading_advisor::propose(c, &a)?)?;
        }
        [cmd, proposal, data, plan, minimum, out] if cmd == "evaluate" => {
            let (_, e) = evidence(proposal, data, plan, minimum)?;
            write(out, &e)?;
        }
        [cmd, proposal, data, plan, minimum, reviewer, out] if cmd == "approve" => {
            // Refuse automation before reading dataset or proposed instructions.
            ensure!(
                std::io::stdin().is_terminal(),
                "approval requires an interactive operator terminal"
            );
            ensure!(
                !reviewer.trim().is_empty() && reviewer.len() <= 128,
                "invalid reviewer"
            );
            let (p, e) = evidence(proposal, data, plan, minimum)?;
            ensure!(
                e.eligible_for_manual_review,
                "insufficient evidence; approval blocked"
            );
            let digest = hash(&e)?;
            println!(
                "PAPER research only. Review the evaluated report, costs, sample size and holdouts. No deployment or profitability guarantee.\nEvidence: {digest}\nType APPROVE {digest} to record your review:"
            );
            std::io::stdout().flush()?;
            let mut confirmation = String::new();
            std::io::stdin().read_line(&mut confirmation)?;
            ensure!(
                confirmation.trim() == format!("APPROVE {digest}"),
                "approval not confirmed"
            );
            write(
                out,
                &Approval {
                    proposal_id: p.id,
                    evidence_hash: digest,
                    reviewer: reviewer.clone(),
                    decision: "approve_for_paper_review".into(),
                    reviewed_at_ms: trading_domain::data::now_ms(),
                },
            )?;
        }
        [cmd, proposal, data, plan, minimum, approval, out] if cmd == "export" => {
            // Recompute evidence: a forged report cannot skip the backtest.
            let (p, e) = evidence(proposal, data, plan, minimum)?;
            let approval: Approval = read(approval, 4096)?;
            write(out, &trading_advisor::reviewed_strategy(&p, &e, &approval)?)?;
        }
        _ => anyhow::bail!(
            "usage: trading-advisor prompt CONTEXT NEW_PROMPT | propose CONTEXT ADVICE NEW_PROPOSAL | evaluate PROPOSAL DATASET EXPERIMENT MIN_HOLDOUT_TRADES NEW_REPORT | approve PROPOSAL DATASET EXPERIMENT MIN_HOLDOUT_TRADES REVIEWER NEW_APPROVAL | export PROPOSAL DATASET EXPERIMENT MIN_HOLDOUT_TRADES APPROVAL NEW_STRATEGY"
        ),
    }
    Ok(())
}
