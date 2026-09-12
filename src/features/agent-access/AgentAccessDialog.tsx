import {
  ArrowClockwiseIcon,
  CopyIcon,
  PlugsConnectedIcon,
  ShieldCheckIcon,
  TrashIcon,
  WarningOctagonIcon,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Dialog } from "../../components/ui";
import {
  approveAgentPairing,
  applyAgentSetup,
  applyAgentSkill,
  decideAgentApproval,
  denyAgentPairing,
  getAgentAccessStatus,
  getAgentSetupStatus,
  getAgentSkillStatus,
  listAgentApprovals,
  planAgentSetup,
  planAgentSkill,
  revokeAgentClient,
  setAgentAccessEnabled,
  setAgentProjectGrant,
  type ActiveProject,
  type AgentAccessStatus,
  type AgentApproval,
  type AgentClient,
  type AgentHostInstallation,
  type AgentHostState,
  type AgentProjectGrant,
  type AgentSetupOperation,
  type AgentSetupPlan,
  type AgentSetupStatus,
  type AgentSkillOperation,
  type AgentSkillPlan,
  type AgentSkillStatus,
} from "../../lib/commands";
import { ExportDestinations } from "./ExportDestinations";
import "./agent-access.css";

interface AgentAccessDialogProps {
  project: ActiveProject | null;
}

const EMPTY_STATUS: AgentAccessStatus = {
  enabled: false,
  endpointReady: false,
  pairedClients: [],
  pendingPairings: [],
  connectedClients: 0,
};

const EMPTY_SETUP: AgentSetupStatus = {
  platform: "unknown",
  packagedServerReady: false,
  hosts: [],
};

const EMPTY_SKILL: AgentSkillStatus = {
  host: "pi",
  hostName: "Pi",
  state: "unsupported",
  detail: "MCP prompts and packaged manual guidance remain available.",
  canInstall: false,
  canRemove: false,
};

function hostStateLabel(state: AgentHostState): string {
  if (state === "not_installed") return "Not installed";
  if (state === "not_configured") return "Not configured";
  if (state === "restart_required") return "Restart required";
  if (state === "repair_required") return "Repair required";
  if (state === "conflict") return "Conflict";
  if (state === "configured") return "Configured";
  return "Guided setup";
}

function setupAction(host: AgentHostInstallation): AgentSetupOperation {
  return host.state === "repair_required" ? "repair" : "configure";
}

export function AgentAccessDialog({ project }: AgentAccessDialogProps) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const setupTriggerRef = useRef<HTMLButtonElement>(null);
  const skillTriggerRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [setupReviewOpen, setSetupReviewOpen] = useState(false);
  const [skillReviewOpen, setSkillReviewOpen] = useState(false);
  const [status, setStatus] = useState<AgentAccessStatus>(EMPTY_STATUS);
  const [setup, setSetup] = useState<AgentSetupStatus>(EMPTY_SETUP);
  const [skill, setSkill] = useState<AgentSkillStatus>(EMPTY_SKILL);
  const [setupPlan, setSetupPlan] = useState<AgentSetupPlan | null>(null);
  const [skillPlan, setSkillPlan] = useState<AgentSkillPlan | null>(null);
  const [setupError, setSetupError] = useState<string | null>(null);
  const [skillError, setSkillError] = useState<string | null>(null);
  const [setupMessage, setSetupMessage] = useState<string | null>(null);
  const [approvals, setApprovals] = useState<AgentApproval[]>([]);
  const [confirmations, setConfirmations] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [setupLoading, setSetupLoading] = useState(false);
  const [setupBusy, setSetupBusy] = useState(false);
  const [skillBusy, setSkillBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [nextStatus, nextApprovals] = await Promise.all([
        getAgentAccessStatus(),
        listAgentApprovals(),
      ]);
      setStatus(nextStatus);
      setApprovals(nextApprovals);
      setError(null);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setLoading(false);
    }
  }, []);

  const refreshSetup = useCallback(async () => {
    setSetupLoading(true);
    try {
      const [nextSetup, nextSkill] = await Promise.all([
        getAgentSetupStatus(),
        getAgentSkillStatus(),
      ]);
      setSetup(nextSetup ?? EMPTY_SETUP);
      setSkill(nextSkill ?? EMPTY_SKILL);
      setError(null);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSetupLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    const initial = window.setTimeout(() => {
      void refresh();
      void refreshSetup();
    }, 0);
    const interval = window.setInterval(() => void refresh(), 1_000);
    return () => {
      window.clearTimeout(initial);
      window.clearInterval(interval);
    };
  }, [open, refresh, refreshSetup]);

  async function run(operation: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await operation();
      await refresh();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  }

  async function reviewSetup(
    host: AgentHostInstallation,
    operation: AgentSetupOperation,
    trigger: HTMLButtonElement,
  ) {
    if (busy || setupBusy) return;
    setupTriggerRef.current = trigger;
    setSetupPlan(null);
    setSetupError(null);
    setSetupMessage(null);
    setSetupReviewOpen(true);
    setSetupBusy(true);
    try {
      setSetupPlan(await planAgentSetup(host.kind, operation));
    } catch (nextError) {
      setSetupError(String(nextError));
    } finally {
      setSetupBusy(false);
    }
  }

  async function reviewSkill(operation: AgentSkillOperation, trigger: HTMLButtonElement) {
    if (busy || skillBusy) return;
    skillTriggerRef.current = trigger;
    setSkillPlan(null);
    setSkillError(null);
    setSkillReviewOpen(true);
    setSkillBusy(true);
    try {
      setSkillPlan(await planAgentSkill(skill.host, operation));
    } catch (nextError) {
      setSkillError(String(nextError));
    } finally {
      setSkillBusy(false);
    }
  }

  async function applySkillPlan() {
    if (!skillPlan || skillBusy) return;
    setSkillBusy(true);
    setSkillError(null);
    try {
      await applyAgentSkill(skillPlan.planId);
      setSkillReviewOpen(false);
      setSkillPlan(null);
      await refreshSetup();
    } catch (nextError) {
      setSkillError(String(nextError));
    } finally {
      setSkillBusy(false);
    }
  }

  async function applySetupPlan() {
    if (!setupPlan || setupPlan.setupMethod === "guided" || setupBusy) return;
    setSetupBusy(true);
    setSetupError(null);
    try {
      const result = await applyAgentSetup(setupPlan.planId);
      setSetupMessage(result.message);
      setSetupReviewOpen(false);
      setSetupPlan(null);
      await refreshSetup();
    } catch (nextError) {
      setSetupError(String(nextError));
    } finally {
      setSetupBusy(false);
    }
  }

  async function copySetupCommand() {
    if (!setupPlan?.commandPreview) return;
    setSetupError(null);
    try {
      await navigator.clipboard.writeText(setupPlan.commandPreview);
      setSetupMessage("Setup command copied. Paste it into the host's reviewed MCP setup.");
    } catch {
      setSetupError("The command could not be copied. Select it from this guide instead.");
    }
  }

  return (
    <Dialog
      busy={busy}
      contentClassName="agent-access-dialog"
      description="Pair local MCP clients and choose exactly which active project capabilities they receive. Tarik stays open and owns every operation."
      onOpenChange={setOpen}
      open={open}
      returnFocusRef={triggerRef}
      title="Agent access"
      trigger={
        <button className="text-button agent-access-trigger" ref={triggerRef} type="button">
          Agent access
        </button>
      }
    >
      <div className="agent-access-summary">
        <div>
          <span className="agent-access-state-label">Local gateway</span>
          <strong>{status.enabled ? "Enabled" : "Off"}</strong>
          <small>
            {status.enabled
              ? status.endpointReady
                ? `${status.connectedClients} authenticated connection${status.connectedClients === 1 ? "" : "s"}`
                : "The local endpoint could not start"
              : "No MCP client can connect"}
          </small>
        </div>
        <button
          aria-pressed={status.enabled}
          className={`agent-access-toggle ${
            status.enabled ? "agent-access-toggle-disable" : "agent-access-toggle-enable"
          }`}
          disabled={busy || loading}
          onClick={() => void run(() => setAgentAccessEnabled(!status.enabled))}
          type="button"
        >
          {status.enabled ? "Disable" : "Enable"}
        </button>
      </div>

      {error && (
        <div className="ui-inline-error" role="alert">
          <strong>Agent access could not update</strong>
          <span>{error}</span>
        </div>
      )}

      <section className="agent-access-section" aria-labelledby="connection-heading">
        <header>
          <div>
            <h3 id="connection-heading">Connect an agent</h3>
            <p>Register Tarik with a host. Pairing and project access remain separate.</p>
          </div>
          <button
            aria-label="Refresh detected agent hosts"
            className="icon-button"
            disabled={busy || setupLoading}
            onClick={() => void refreshSetup()}
            type="button"
          >
            <ArrowClockwiseIcon aria-hidden="true" size={15} />
          </button>
        </header>
        {!setup.packagedServerReady && !setupLoading && (
          <p className="agent-setup-warning" role="status">
            Packaged tarik-mcp is unavailable. Host configuration is disabled.
          </p>
        )}
        {setupLoading && setup.hosts.length === 0 ? (
          <p className="agent-access-empty" role="status">
            Checking supported hosts…
          </p>
        ) : (
          <div className="agent-host-list">
            {setup.hosts.map((host) => (
              <article className="agent-host-row" key={host.kind}>
                <div className="agent-host-copy">
                  <div>
                    <strong>{host.displayName}</strong>
                    <span className={`agent-host-state agent-host-state-${host.state}`}>
                      {hostStateLabel(host.state)}
                    </span>
                  </div>
                  <p>{host.detail}</p>
                  {host.version && <small>{host.version}</small>}
                </div>
                <div className="agent-host-actions">
                  {host.canRemove && (
                    <button
                      className="toolbar-button"
                      disabled={busy || setupBusy}
                      onClick={(event) => void reviewSetup(host, "remove", event.currentTarget)}
                      type="button"
                    >
                      Remove
                    </button>
                  )}
                  {host.canConfigure && host.state !== "configured" && (
                    <button
                      className={host.setupMethod === "guided" ? "toolbar-button" : "run-button"}
                      disabled={busy || setupBusy}
                      onClick={(event) =>
                        void reviewSetup(host, setupAction(host), event.currentTarget)
                      }
                      type="button"
                    >
                      {host.state === "repair_required"
                        ? "Review repair"
                        : host.setupMethod === "guided"
                          ? "Show steps"
                          : "Review setup"}
                    </button>
                  )}
                </div>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="agent-access-section" aria-labelledby="guidance-heading">
        <header>
          <div>
            <h3 id="guidance-heading">Optional workflow guidance</h3>
            <p>Teach compatible agents the guarded Tarik workflow without granting authority.</p>
          </div>
        </header>
        <article className="agent-host-row">
          <div className="agent-host-copy">
            <div>
              <strong>Tarik MCP skill for {skill.hostName}</strong>
              <span className={`agent-host-state agent-host-state-${skill.state}`}>
                {skill.state === "installed"
                  ? "Installed"
                  : skill.state === "available"
                    ? "Available"
                    : skill.state === "conflict"
                      ? "Conflict"
                      : "Manual guidance"}
              </span>
            </div>
            <p>{skill.detail}</p>
          </div>
          <div className="agent-host-actions">
            {skill.canRemove && (
              <button
                className="toolbar-button"
                disabled={busy || skillBusy}
                onClick={(event) => void reviewSkill("remove", event.currentTarget)}
                type="button"
              >
                Review removal
              </button>
            )}
            {skill.canInstall && (
              <button
                className="toolbar-button"
                disabled={busy || skillBusy}
                onClick={(event) => void reviewSkill("install", event.currentTarget)}
                type="button"
              >
                Review skill
              </button>
            )}
          </div>
        </article>
      </section>

      <section className="agent-access-section" aria-labelledby="pairing-heading">
        <header>
          <div>
            <h3 id="pairing-heading">Pairing requests</h3>
            <p>A new client remains disconnected until you accept it here.</p>
          </div>
          <span>{status.pendingPairings.length}/4</span>
        </header>
        {loading && status.pendingPairings.length === 0 ? (
          <p className="agent-access-empty" role="status">
            Checking local clients…
          </p>
        ) : status.pendingPairings.length === 0 ? (
          <p className="agent-access-empty">No pending requests.</p>
        ) : (
          <div className="agent-access-list">
            {status.pendingPairings.map((pairing) => (
              <div className="agent-pairing-row" key={pairing.id}>
                <PlugsConnectedIcon aria-hidden="true" size={18} />
                <div>
                  <strong>{pairing.displayName}</strong>
                  <span>Expires in {pairing.expiresInSeconds} seconds</span>
                </div>
                <button
                  className="toolbar-button"
                  disabled={busy}
                  onClick={() => void run(() => denyAgentPairing(pairing.id))}
                  type="button"
                >
                  Deny
                </button>
                <button
                  className="run-button"
                  disabled={busy}
                  onClick={() => void run(() => approveAgentPairing(pairing.id))}
                  type="button"
                >
                  Pair client
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="agent-access-section" aria-labelledby="approvals-heading">
        <header>
          <div>
            <h3 id="approvals-heading">Action approvals</h3>
            <p>Review the exact immutable action. MCP clients cannot approve here.</p>
          </div>
          <span>{approvals.length}/16</span>
        </header>
        {approvals.length === 0 ? (
          <p className="agent-access-empty">No actions are waiting for approval.</p>
        ) : (
          <div className="agent-access-list">
            {approvals.map((approval) => {
              const typed = confirmations[approval.id] ?? "";
              const critical = approval.criticalPhrase !== null;
              return (
                <article className="agent-approval-row" key={approval.id}>
                  <header>
                    {critical ? (
                      <WarningOctagonIcon aria-hidden="true" size={19} />
                    ) : (
                      <ShieldCheckIcon aria-hidden="true" size={19} />
                    )}
                    <div>
                      <strong>
                        {approval.action === "export"
                          ? critical
                            ? "Critical export replacement"
                            : "Export policy exception"
                          : critical
                            ? "Critical data change"
                            : "Data change"}
                      </strong>
                      <span>
                        {approval.clientName} · {approval.projectName} · expires in{" "}
                        {approval.expiresInSeconds}s
                      </span>
                    </div>
                  </header>
                  <dl>
                    <div>
                      <dt>{approval.action === "export" ? "Destination" : "Objects"}</dt>
                      <dd>{approval.affectedObjects.join(", ") || "Impact unknown"}</dd>
                    </div>
                    <div>
                      <dt>Filter</dt>
                      <dd>
                        {approval.hasTopLevelFilter === null
                          ? "Not applicable"
                          : approval.hasTopLevelFilter
                            ? "Top-level filter present"
                            : "No provable top-level filter"}
                      </dd>
                    </div>
                  </dl>
                  <pre aria-label="Exact SQL snapshot">{approval.sql}</pre>
                  <small>Snapshot {approval.snapshotHash.slice(0, 12)}</small>
                  {approval.criticalPhrase && (
                    <label className="agent-critical-confirmation">
                      <span>
                        Type <strong>{approval.criticalPhrase}</strong> to enable approval
                      </span>
                      <input
                        autoComplete="off"
                        onChange={(event) => {
                          const value = event.currentTarget.value;
                          setConfirmations((current) => ({ ...current, [approval.id]: value }));
                        }}
                        onPaste={(event) => event.preventDefault()}
                        spellCheck={false}
                        value={typed}
                      />
                    </label>
                  )}
                  <div className="agent-approval-actions">
                    <button
                      className="toolbar-button"
                      disabled={busy}
                      onClick={() => void run(() => decideAgentApproval(approval.id, false, null))}
                      type="button"
                    >
                      Deny
                    </button>
                    <button
                      className="run-button"
                      disabled={busy || (critical && typed !== approval.criticalPhrase)}
                      onClick={() =>
                        void run(() =>
                          decideAgentApproval(approval.id, true, critical ? typed : null),
                        )
                      }
                      type="button"
                    >
                      Approve once
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </section>

      <section className="agent-access-section" aria-labelledby="clients-heading">
        <header>
          <div>
            <h3 id="clients-heading">Paired clients</h3>
            <p>New clients receive no project access. Grants apply only to the active project.</p>
          </div>
          <span>{status.pairedClients.length}/8</span>
        </header>
        {status.pairedClients.length === 0 ? (
          <p className="agent-access-empty">No clients are paired.</p>
        ) : (
          <div className="agent-access-list">
            {status.pairedClients.map((client) => (
              <ClientPermissions
                busy={busy}
                client={client}
                key={client.id}
                onChange={(grant) => run(() => setAgentProjectGrant(client.id, grant))}
                onRevoke={() => run(() => revokeAgentClient(client.id))}
                project={project}
              />
            ))}
          </div>
        )}
      </section>

      <div className="agent-access-note">
        <ShieldCheckIcon aria-hidden="true" size={18} />
        <p>
          MCP-host confirmations do not approve Tarik actions. Data or workspace changes still
          require a separate one-use approval inside Tarik.
        </p>
      </div>

      <Dialog
        busy={setupBusy}
        contentClassName="agent-setup-dialog"
        description="Review the exact host configuration instructions. This does not pair the client or grant project access."
        onOpenChange={(nextOpen) => {
          setSetupReviewOpen(nextOpen);
          if (!nextOpen) {
            setSetupPlan(null);
            setSetupError(null);
          }
        }}
        open={setupReviewOpen}
        returnFocusRef={setupTriggerRef}
        title={setupPlan ? `Connect ${setupPlan.hostName}` : "Preparing setup guide"}
      >
        <div className="agent-setup-plan">
          {setupBusy && !setupPlan ? (
            <p className="agent-access-empty" role="status">
              Preparing reviewed setup…
            </p>
          ) : setupPlan ? (
            <>
              <header>
                <div>
                  <strong>{setupPlan.hostName}</strong>
                  <span>
                    {setupPlan.setupMethod === "guided" ? "Guided setup" : "Ready for local apply"}
                  </span>
                </div>
              </header>
              <p>{setupPlan.summary}</p>
              {setupPlan.configTarget && (
                <dl>
                  <dt>Configuration</dt>
                  <dd>{setupPlan.configTarget}</dd>
                </dl>
              )}
              {setupPlan.commandPreview && (
                <pre aria-label="Exact setup command">{setupPlan.commandPreview}</pre>
              )}
              <small>
                Setup changes host configuration only. Pairing and project grants remain separate
                actions inside Tarik.
              </small>
            </>
          ) : null}

          {setupError && (
            <div className="ui-inline-error" role="alert">
              <strong>Setup could not continue</strong>
              <span>{setupError}</span>
            </div>
          )}

          {setupMessage && (
            <p className="agent-setup-message" role="status">
              {setupMessage}
            </p>
          )}

          <div className="agent-setup-actions">
            <button
              className="toolbar-button"
              disabled={setupBusy}
              onClick={() => setSetupReviewOpen(false)}
              type="button"
            >
              Close
            </button>
            {setupPlan?.setupMethod === "guided" ? (
              <button
                className="run-button"
                disabled={setupBusy || !setupPlan.commandPreview}
                onClick={() => void copySetupCommand()}
                type="button"
              >
                <CopyIcon aria-hidden="true" size={14} />
                Copy command
              </button>
            ) : setupPlan ? (
              <button
                className={setupPlan.operation === "remove" ? "danger-button" : "run-button"}
                disabled={setupBusy}
                onClick={() => void applySetupPlan()}
                type="button"
              >
                {setupPlan.operation === "remove" ? "Remove configuration" : "Apply setup"}
              </button>
            ) : null}
          </div>
        </div>
      </Dialog>

      <Dialog
        busy={skillBusy}
        contentClassName="agent-setup-dialog"
        description="Review an optional guidance-only Agent Skill. This cannot pair a client, grant a project, approve an action, or weaken Tarik policy."
        onOpenChange={(nextOpen) => {
          setSkillReviewOpen(nextOpen);
          if (!nextOpen) {
            setSkillPlan(null);
            setSkillError(null);
          }
        }}
        open={skillReviewOpen}
        returnFocusRef={skillTriggerRef}
        title={
          skillPlan
            ? `${skillPlan.operation === "install" ? "Install" : "Remove"} Tarik skill`
            : "Preparing skill review"
        }
      >
        <div className="agent-setup-plan">
          {skillBusy && !skillPlan ? (
            <p className="agent-access-empty" role="status">
              Preparing reviewed guidance change…
            </p>
          ) : skillPlan ? (
            <>
              <header>
                <div>
                  <strong>{skillPlan.hostName}</strong>
                  <span>Guidance only</span>
                </div>
              </header>
              <p>{skillPlan.summary}</p>
              <dl>
                <dt>Skill target</dt>
                <dd>{skillPlan.target}</dd>
              </dl>
              <small>
                Tarik writes or removes only its exact reviewed skill. Pairing, project grants,
                approvals, queries, and exports remain separate backend-controlled actions.
              </small>
            </>
          ) : null}

          {skillError && (
            <div className="ui-inline-error" role="alert">
              <strong>Guidance could not change</strong>
              <span>{skillError}</span>
            </div>
          )}

          <div className="agent-setup-actions">
            <button
              className="toolbar-button"
              disabled={skillBusy}
              onClick={() => setSkillReviewOpen(false)}
              type="button"
            >
              Close
            </button>
            {skillPlan && (
              <button
                className={skillPlan.operation === "remove" ? "danger-button" : "run-button"}
                disabled={skillBusy}
                onClick={() => void applySkillPlan()}
                type="button"
              >
                {skillPlan.operation === "remove" ? "Remove skill" : "Install skill"}
              </button>
            )}
          </div>
        </div>
      </Dialog>
    </Dialog>
  );
}

function ClientPermissions({
  busy,
  client,
  onChange,
  onRevoke,
  project,
}: {
  busy: boolean;
  client: AgentClient;
  onChange: (grant: AgentProjectGrant) => void | Promise<unknown>;
  onRevoke: () => void | Promise<unknown>;
  project: ActiveProject | null;
}) {
  const current = project
    ? client.grants.find((grant) => grant.projectId === project.id)
    : undefined;
  const grant: AgentProjectGrant = current ?? {
    projectId: project?.id ?? "",
    inspect: false,
    analyze: false,
    modifyWorkspace: false,
    modifyData: false,
  };

  function update(patch: Partial<AgentProjectGrant>) {
    if (!project) return;
    const next = { ...grant, ...patch, projectId: project.id };
    if (next.modifyData) {
      next.modifyWorkspace = true;
      next.analyze = true;
      next.inspect = true;
    }
    if (next.modifyWorkspace || next.analyze) next.inspect = true;
    void onChange(next);
  }

  return (
    <article className="agent-client-row">
      <header>
        <div>
          <strong>{client.displayName}</strong>
          <span>{client.connected ? "Connected" : "Not connected"}</span>
        </div>
        <button
          aria-label={`Revoke ${client.displayName}`}
          className="icon-button"
          disabled={busy}
          onClick={() => void onRevoke()}
          type="button"
        >
          <TrashIcon aria-hidden="true" size={15} />
        </button>
      </header>
      {project ? (
        <fieldset disabled={busy}>
          <legend>{project.name}</legend>
          <label>
            <input
              checked={grant.inspect}
              onChange={(event) => {
                const checked = event.currentTarget.checked;
                update({ inspect: checked });
              }}
              type="checkbox"
            />
            Inspect catalog
          </label>
          <label>
            <input
              checked={grant.analyze}
              onChange={(event) => {
                const checked = event.currentTarget.checked;
                update({ analyze: checked });
              }}
              type="checkbox"
            />
            Analyze data
          </label>
          <label>
            <input
              checked={grant.modifyWorkspace}
              onChange={(event) => {
                const checked = event.currentTarget.checked;
                update({ modifyWorkspace: checked });
              }}
              type="checkbox"
            />
            Modify workspace
          </label>
          <label>
            <input
              checked={grant.modifyData}
              onChange={(event) => {
                const checked = event.currentTarget.checked;
                update({ modifyData: checked });
              }}
              type="checkbox"
            />
            Modify data
          </label>
        </fieldset>
      ) : (
        <p className="agent-access-empty">Open a project to edit its grants.</p>
      )}
      {project && grant.analyze && (
        <ExportDestinations client={client} disabled={busy} project={project} />
      )}
    </article>
  );
}
