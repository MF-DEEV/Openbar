// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// Codex's banked resets (NotchAgentsView.swift:842-1003; spec 03 §3.4): how
// many the account holds, when the next expires, and a use that asks first.
// Cancel takes the place of the button that asked, so a double click never
// spends one, and a count that changes while asking asks again.

import { CircleAlert, CircleCheck, Info, LoaderCircle, RotateCcw } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { bridge } from "../shared/bridge";
import { PROVIDER_TINTS, relative, LIMIT_ORANGE } from "./format";
import { Card, CardHeader, Chip } from "./Parts";
import { A } from "./strings";
import type { CodexFailure, RedeemOutcome, ResetsState } from "./types";

const TINT = PROVIDER_TINTS.codex;
/** What a use did stays on the card for a minute. */
const OUTCOME_SHOWN = 60_000;

export function ResetsCard({ resets, now }: { resets: ResetsState; now: number }) {
  const [confirming, setConfirming] = useState(false);
  const [redeeming, setRedeeming] = useState(false);
  const [finished, setFinished] = useState<{ outcome: RedeemOutcome; at: number } | null>(null);

  useEffect(() => {
    void bridge.agents.refreshResets(false);
  }, []);
  // A count that changed while the question was open asks again.
  useEffect(() => setConfirming(false), [resets.count]);

  const redeem = async () => {
    setConfirming(false);
    setRedeeming(true);
    try {
      const outcome = (await bridge.agents.redeemReset()) ?? "failed";
      setFinished({ outcome, at: Date.now() });
    } catch {
      setFinished({ outcome: "failed", at: Date.now() });
    } finally {
      setRedeeming(false);
    }
  };

  const available = resets.count ?? 0;
  const shownOutcome = finished && now - finished.at < OUTCOME_SHOWN ? finished.outcome : null;
  const useButton = (
    <Pill icon onClick={() => setConfirming(true)}>
      {A.useReset}
    </Pill>
  );

  let content: ReactNode;
  if (redeeming) {
    content = <Line icon={<LoaderCircle className="ag-spin" size={11} aria-hidden />} text={A.resetting} plain />;
  } else if (confirming && available > 0) {
    content = (
      <Action message={A.resetConfirm} emphasized>
        <Pill prominent={false} onClick={() => setConfirming(false)}>
          {A.cancel}
        </Pill>
        <Pill onClick={() => void redeem()}>{A.confirmReset}</Pill>
      </Action>
    );
  } else if (shownOutcome) {
    content =
      shownOutcome === "failed" && available > 0 ? (
        // Trying again repeats the same use, which never spends a second reset.
        <Action message="">
          <Outcome outcome={shownOutcome} />
          {useButton}
        </Action>
      ) : (
        <Outcome outcome={shownOutcome} />
      );
  } else if (resets.count === null && resets.failure) {
    content = (
      <Action message={failureText(resets.failure)}>
        <Pill prominent={false} onClick={() => void bridge.agents.refreshResets(true)}>
          {A.retry}
        </Pill>
      </Action>
    );
  } else if (resets.count !== null) {
    if (available > 0) {
      const next = resets.credits[0]?.expiresAt ?? null;
      // A reset a day from expiring is worth using soon.
      const soon = next !== null && next - now < 86_400_000;
      content = (
        <Action
          message={next !== null ? A.resetsExpiry(relative(next, now, "long")) : ""}
          tint={soon ? LIMIT_ORANGE : undefined}
        >
          {useButton}
        </Action>
      );
    } else content = <Line text={A.resetsNone} />;
  }

  return (
    <Card title={A.resetsHelp}>
      <CardHeader title={A.resetsCard} provider="codex" tint={TINT}>
        {available > 0 ? (
          <Chip tint={TINT}>{available}</Chip>
        ) : resets.checking ? (
          <LoaderCircle className="ag-spin" size={11} color="rgba(255,255,255,0.6)" aria-label={A.resetting} />
        ) : null}
      </CardHeader>
      {content}
    </Card>
  );
}

function failureText(failure: CodexFailure): string {
  switch (failure) {
    case "missing":
      return A.resetsNeedCodex;
    case "needsSignIn":
      return A.resetsSignIn;
    case "outdated":
      return A.resetsUpdate;
    default:
      return A.resetsCheckFailed;
  }
}

function Outcome({ outcome }: { outcome: RedeemOutcome }) {
  switch (outcome) {
    case "reset":
      return <Line icon={<CircleCheck size={11} color="#30D158" aria-hidden />} text={A.resetDone} plain />;
    case "nothingToReset":
      return (
        <Line icon={<Info size={11} color="rgba(255,255,255,0.55)" aria-hidden />} text={A.resetNotNeeded} plain />
      );
    case "alreadyRedeemed":
      return <Line icon={<Info size={11} color="rgba(255,255,255,0.55)" aria-hidden />} text={A.resetTaken} plain />;
    case "noCredit":
      return <Line text={A.resetsNone} />;
    case "failed":
      return <Line icon={<CircleAlert size={11} color={LIMIT_ORANGE} aria-hidden />} text={A.resetFailed} plain />;
  }
}

/** A line of the card's text; with a symbol it reads brighter. */
function Line(props: { text: string; icon?: ReactNode; plain?: boolean }) {
  return (
    <div className={`ag-resets-line${props.plain ? " is-bright" : ""}`}>
      {props.icon}
      <span>{props.text}</span>
    </div>
  );
}

/** A message above the card's buttons, which keep to its bottom edge. */
function Action(props: { message: string; emphasized?: boolean; tint?: string; children: ReactNode }) {
  return (
    <div className="ag-resets-action">
      {props.message ? (
        <div
          className={`ag-resets-message${props.emphasized ? " is-emphasized" : ""}`}
          style={props.tint ? { color: props.tint } : undefined}
        >
          {props.message}
        </div>
      ) : null}
      <span className="ag-spacer-v" />
      <div className="ag-resets-buttons">{props.children}</div>
    </div>
  );
}

/** 20 tall, padding 9, Codex's tint at 20 % when prominent, white 10 % otherwise. */
function Pill(props: { children: ReactNode; prominent?: boolean; icon?: boolean; onClick(): void }) {
  const prominent = props.prominent ?? true;
  return (
    <button
      type="button"
      className={`ag-pill${prominent ? " is-prominent" : ""}`}
      style={prominent ? { color: TINT } : undefined}
      onClick={props.onClick}
    >
      {props.icon ? <RotateCcw size={9} strokeWidth={2.75} aria-hidden /> : null}
      {props.children}
    </button>
  );
}
