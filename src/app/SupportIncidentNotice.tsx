import { CopyIcon, FolderOpenIcon } from "@phosphor-icons/react";
import { useState } from "react";
import { revealLogDirectory, type SupportIncident } from "../lib/commands";

interface SupportIncidentNoticeProps {
  incident: SupportIncident;
  heading?: string;
  onDismiss?: () => void;
  onRetry?: () => void;
}

export function SupportIncidentNotice({
  incident,
  heading = "Tarik needs your attention",
  onDismiss,
  onRetry,
}: SupportIncidentNoticeProps) {
  const [copied, setCopied] = useState(false);

  async function copyIncidentId() {
    try {
      await navigator.clipboard.writeText(incident.incidentId);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <section className="support-incident" role="alert">
      <div className="support-incident-copy">
        <strong>{heading}</strong>
        <p>{incident.summary}</p>
        <div className="support-incident-id">
          <span>Incident ID</span>
          <code>{incident.incidentId}</code>
        </div>
        <p className="support-incident-note">
          {incident.loggingSucceeded
            ? "Support details were written locally. Query text and result rows are excluded from diagnostic logs."
            : "The incident could not be written to the log file. Copy the ID before closing Tarik."}
        </p>
      </div>
      <div className="support-incident-actions">
        <button className="text-button" onClick={() => void copyIncidentId()} type="button">
          <CopyIcon aria-hidden="true" size={14} />
          {copied ? "Copied" : "Copy ID"}
        </button>
        <button className="text-button" onClick={() => void revealLogDirectory()} type="button">
          <FolderOpenIcon aria-hidden="true" size={14} />
          Reveal logs
        </button>
        {onRetry && (
          <button className="text-button" onClick={onRetry} type="button">
            Retry
          </button>
        )}
        {onDismiss && (
          <button className="text-button" onClick={onDismiss} type="button">
            Dismiss
          </button>
        )}
      </div>
    </section>
  );
}
