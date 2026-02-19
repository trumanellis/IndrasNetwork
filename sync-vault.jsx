import { useState, useEffect, useRef, useCallback } from "react";

// ═══════════════════════════════════════════════════════════════════
//  DATA
// ═══════════════════════════════════════════════════════════════════

const PEERS = {
  self:   { name: "You",    color: "#e8a849" },
  nova:   { name: "Nova",   color: "#6bc5e8" },
  sage:   { name: "Sage",   color: "#8be86b" },
  ember:  { name: "Ember",  color: "#e86b8b" },
  zephyr: { name: "Zephyr", color: "#c59be8" },
};

const TYPES = {
  conversation: { icon: "◎", label: "Conversation" },
  gallery:      { icon: "▣", label: "Gallery" },
  request:      { icon: "✦", label: "Request" },
  document:     { icon: "▤", label: "Document" },
  exchange:     { icon: "⇋", label: "Exchange" },
  message:      { icon: "◯", label: "Message" },
  image:        { icon: "▢", label: "Image" },
};

// Your artifacts — things you steward, synced out to audience
const OUTGOING = [
  {
    id: "conv-agua", type: "conversation", name: "Agua Lila Community",
    audience: ["nova", "sage", "ember", "zephyr"],
    syncedWith: ["nova", "sage", "ember"], pendingSync: ["zephyr"],
    heat: 0.9, lastActivity: "45m ago",
    preview: "4 threads · latest: you offered to clear debris",
    children: 8,
  },
  {
    id: "gallery-builds", type: "gallery", name: "Building Archive",
    audience: ["nova", "ember", "sage"],
    syncedWith: ["nova", "ember", "sage"], pendingSync: [],
    heat: 0.4, lastActivity: "6h ago",
    preview: "12 images of natural building techniques",
    children: 12,
  },
  {
    id: "req-compost", type: "request", name: "Compost System Design",
    audience: ["sage", "zephyr", "ember"],
    syncedWith: ["sage", "zephyr"], pendingSync: ["ember"],
    heat: 0.65, lastActivity: "2h ago",
    preview: "Your request · 2 offers received",
    children: 2,
  },
  {
    id: "dm-nova", type: "conversation", name: "Nova (DM)",
    audience: ["nova"],
    syncedWith: ["nova"], pendingSync: [],
    heat: 0.35, lastActivity: "3h ago",
    preview: "Discussing the bioregional mapping tool",
    children: 2,
  },
];

// Incoming artifacts — things peers steward, synced to you
const INCOMING = [
  {
    id: "doc-protocol", type: "document", name: "Regen Protocol",
    from: "nova",
    syncStatus: "synced", lastSync: "2m ago",
    heat: 0.25, lastActivity: "2d ago",
    preview: "Nova updated section 3 — water rights framework",
    fresh: false,
  },
  {
    id: "exchange-seeds", type: "exchange", name: "Seed Swap",
    from: "ember",
    syncStatus: "synced", lastSync: "12m ago",
    heat: 0.55, lastActivity: "1d ago",
    preview: "Ember offers lavender starts for your tomato seeds",
    fresh: true,
    action: "Accept or decline",
  },
  {
    id: "req-workshop", type: "request", name: "Cob Workshop",
    from: "sage",
    syncStatus: "syncing", lastSync: null,
    heat: 0.12, lastActivity: "3d ago",
    preview: "Sage looking for co-facilitator for April workshop",
    fresh: true,
  },
  {
    id: "gallery-watershed", type: "gallery", name: "Watershed Maps",
    from: "nova",
    syncStatus: "synced", lastSync: "1h ago",
    heat: 0.3, lastActivity: "5h ago",
    preview: "7 bioregional maps of the Montanhas Magicas",
    fresh: false,
  },
  {
    id: "msg-sage-invite", type: "message", name: "Gathering Invite",
    from: "sage",
    syncStatus: "synced", lastSync: "30m ago",
    heat: 0.5, lastActivity: "5h ago",
    preview: "Solstice celebration on the 21st — potluck?",
    fresh: true,
    action: "Reply",
  },
];

// Online peers
const ONLINE_PEERS = [
  { id: "nova", lastSeen: "now", syncing: true },
  { id: "sage", lastSeen: "now", syncing: false },
  { id: "ember", lastSeen: "5m ago", syncing: false },
];

const OFFLINE_PEERS = [
  { id: "zephyr", lastSeen: "2h ago" },
];

// ═══════════════════════════════════════════════════════════════════
//  COMPONENTS
// ═══════════════════════════════════════════════════════════════════

function SyncDot({ status, size = 6 }) {
  const colors = {
    synced: "#8be86b",
    syncing: "#e8a849",
    pending: "#5a5650",
    offline: "#2a2820",
  };
  return (
    <div style={{
      width: size, height: size, borderRadius: "50%",
      background: colors[status] || colors.pending,
      boxShadow: status === "syncing" ? `0 0 6px ${colors.syncing}66` : "none",
      animation: status === "syncing" ? "syncPulse 1.5s ease infinite" : "none",
      flexShrink: 0,
    }} />
  );
}

function PeerChip({ peerId, synced = true, showName = true }) {
  const p = PEERS[peerId];
  if (!p) return null;
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 4,
      padding: "2px 0",
    }}>
      <div style={{
        width: 6, height: 6, borderRadius: "50%",
        background: p.color,
        opacity: synced ? 0.8 : 0.3,
      }} />
      {showName && (
        <span style={{
          fontSize: 11, color: p.color,
          opacity: synced ? 0.7 : 0.35,
          fontFamily: "var(--sans)",
        }}>{p.name}</span>
      )}
    </div>
  );
}

function AudienceBar({ synced, pending }) {
  const total = synced.length + pending.length;
  if (total === 0) return null;
  const pct = total > 0 ? (synced.length / total) * 100 : 0;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <div style={{
        flex: 1, height: 3, borderRadius: 2,
        background: "#1a1816", overflow: "hidden",
        maxWidth: 80,
      }}>
        <div style={{
          height: "100%", borderRadius: 2,
          width: `${pct}%`,
          background: pct === 100 ? "#8be86b" : "#e8a849",
          opacity: 0.6,
          transition: "width 0.4s ease",
        }} />
      </div>
      <span style={{
        fontSize: 10, fontFamily: "var(--mono)",
        color: pct === 100 ? "#8be86b88" : "#e8a84988",
      }}>
        {synced.length}/{total}
      </span>
    </div>
  );
}

function OutgoingCard({ artifact, onEnter, delay = 0 }) {
  const [hovered, setHovered] = useState(false);
  const cfg = TYPES[artifact.type];
  const allSynced = artifact.pendingSync.length === 0;

  return (
    <div
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={() => onEnter(artifact.id)}
      style={{
        background: hovered ? "#161412" : "#121110",
        border: `1px solid rgba(232, 168, 73, ${hovered ? 0.12 : 0.04})`,
        borderRadius: 10,
        padding: "14px 16px",
        cursor: "pointer",
        transition: "all 0.2s ease",
        transform: hovered ? "translateY(-1px)" : "none",
        opacity: 0,
        animation: `fadeIn 0.3s ease ${delay}s forwards`,
      }}
    >
      {/* Top row: icon + name + sync status */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 8 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
          <span style={{
            fontSize: 16,
            color: `rgba(232, 168, 73, ${0.3 + artifact.heat * 0.5})`,
            lineHeight: 1,
          }}>{cfg.icon}</span>
          <div style={{ minWidth: 0 }}>
            <div style={{
              fontSize: 14, fontWeight: 500, color: "#d8d0c4",
              fontFamily: "var(--serif)",
              whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            }}>{artifact.name}</div>
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, flexShrink: 0, marginLeft: 8 }}>
          <SyncDot status={allSynced ? "synced" : "syncing"} />
          <span style={{
            fontSize: 10, color: "#4a4640", fontFamily: "var(--mono)",
          }}>{artifact.lastActivity}</span>
        </div>
      </div>

      {/* Preview */}
      <div style={{
        fontSize: 12, color: "#6a6560", lineHeight: 1.45,
        fontFamily: "var(--serif)",
        marginBottom: 10,
      }}>{artifact.preview}</div>

      {/* Audience sync status */}
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "center",
      }}>
        <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap" }}>
          {artifact.syncedWith.map(pid => (
            <PeerChip key={pid} peerId={pid} synced={true} showName={hovered} />
          ))}
          {artifact.pendingSync.map(pid => (
            <PeerChip key={pid} peerId={pid} synced={false} showName={hovered} />
          ))}
        </div>
        <AudienceBar synced={artifact.syncedWith} pending={artifact.pendingSync} />
      </div>
    </div>
  );
}

function IncomingCard({ artifact, onEnter, delay = 0 }) {
  const [hovered, setHovered] = useState(false);
  const cfg = TYPES[artifact.type];
  const peer = PEERS[artifact.from];

  return (
    <div
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={() => onEnter(artifact.id)}
      style={{
        background: hovered ? "#161412" : "#121110",
        border: `1px solid ${artifact.fresh
          ? `rgba(${peer?.color === "#6bc5e8" ? "107,197,232" : peer?.color === "#8be86b" ? "139,232,107" : peer?.color === "#e86b8b" ? "232,107,139" : "232,168,73"}, ${hovered ? 0.18 : 0.08})`
          : `rgba(232, 168, 73, ${hovered ? 0.1 : 0.03})`
        }`,
        borderRadius: 10,
        padding: "14px 16px",
        cursor: "pointer",
        transition: "all 0.2s ease",
        transform: hovered ? "translateY(-1px)" : "none",
        opacity: 0,
        animation: `fadeIn 0.3s ease ${delay}s forwards`,
        position: "relative",
      }}
    >
      {/* Fresh indicator */}
      {artifact.fresh && (
        <div style={{
          position: "absolute", top: 8, right: 8,
          width: 6, height: 6, borderRadius: "50%",
          background: peer?.color || "#e8a849",
          opacity: 0.7,
        }} />
      )}

      {/* Top row */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 8 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
          <span style={{
            fontSize: 16, lineHeight: 1,
            color: `rgba(232, 168, 73, ${0.25 + artifact.heat * 0.45})`,
          }}>{cfg.icon}</span>
          <div style={{ minWidth: 0 }}>
            <div style={{
              fontSize: 14, fontWeight: 500, color: "#d8d0c4",
              fontFamily: "var(--serif)",
              whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
            }}>{artifact.name}</div>
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, flexShrink: 0, marginLeft: 8 }}>
          <SyncDot status={artifact.syncStatus} />
          <span style={{ fontSize: 10, color: "#4a4640", fontFamily: "var(--mono)" }}>
            {artifact.syncStatus === "syncing" ? "syncing…" : artifact.lastSync}
          </span>
        </div>
      </div>

      {/* Preview */}
      <div style={{
        fontSize: 12, color: "#6a6560", lineHeight: 1.45,
        fontFamily: "var(--serif)",
        marginBottom: 8,
      }}>{artifact.preview}</div>

      {/* From + action */}
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "center",
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
          <span style={{ fontSize: 10, color: "#3a3830", fontFamily: "var(--sans)" }}>from</span>
          <PeerChip peerId={artifact.from} showName={true} />
        </div>
        {artifact.action && (
          <span style={{
            fontSize: 10, fontFamily: "var(--sans)", fontWeight: 600,
            color: peer?.color || "#e8a849",
            opacity: 0.6,
            padding: "2px 8px",
            borderRadius: 4,
            background: `${peer?.color || "#e8a849"}0a`,
          }}>{artifact.action}</span>
        )}
      </div>
    </div>
  );
}

function PeerStatusBar() {
  return (
    <div style={{
      display: "flex", gap: 16, alignItems: "center",
      padding: "14px 20px",
      borderBottom: "1px solid rgba(232, 168, 73, 0.03)",
    }}>
      <span style={{ fontSize: 10, color: "#3a3830", fontFamily: "var(--sans)", textTransform: "uppercase", letterSpacing: "0.08em" }}>
        Peers
      </span>
      {ONLINE_PEERS.map(p => {
        const peer = PEERS[p.id];
        return (
          <div key={p.id} style={{ display: "flex", alignItems: "center", gap: 5 }}>
            <div style={{
              width: 7, height: 7, borderRadius: "50%",
              background: peer?.color,
              boxShadow: p.syncing ? `0 0 6px ${peer?.color}44` : "none",
              animation: p.syncing ? "syncPulse 1.5s ease infinite" : "none",
            }} />
            <span style={{
              fontSize: 11, color: peer?.color, opacity: 0.7,
              fontFamily: "var(--sans)", fontWeight: 500,
            }}>{peer?.name}</span>
            {p.syncing && (
              <span style={{ fontSize: 9, color: "#4a4640", fontFamily: "var(--mono)" }}>syncing</span>
            )}
          </div>
        );
      })}
      <div style={{ width: 1, height: 12, background: "#1a1816" }} />
      {OFFLINE_PEERS.map(p => {
        const peer = PEERS[p.id];
        return (
          <div key={p.id} style={{ display: "flex", alignItems: "center", gap: 5, opacity: 0.4 }}>
            <div style={{ width: 7, height: 7, borderRadius: "50%", background: peer?.color, opacity: 0.4 }} />
            <span style={{ fontSize: 11, color: peer?.color, fontFamily: "var(--sans)" }}>{peer?.name}</span>
            <span style={{ fontSize: 9, color: "#3a3830", fontFamily: "var(--mono)" }}>{p.lastSeen}</span>
          </div>
        );
      })}
    </div>
  );
}

function SyncRipple({ cx, cy }) {
  const [ripples, setRipples] = useState([]);
  const id = useRef(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setRipples(prev => [...prev.filter(r => Date.now() - r.t < 3000), { id: id.current++, t: Date.now() }]);
    }, 2500);
    return () => clearInterval(interval);
  }, []);

  return (
    <svg style={{ position: "absolute", inset: 0, pointerEvents: "none", zIndex: 0, width: "100%", height: "100%" }}>
      {ripples.map(r => {
        const age = (Date.now() - r.t) / 3000;
        return (
          <circle
            key={r.id}
            cx={cx} cy={cy}
            r={20 + age * 120}
            fill="none"
            stroke="rgba(232, 168, 73, 0.04)"
            strokeWidth={1 - age * 0.8}
            opacity={1 - age}
          />
        );
      })}
    </svg>
  );
}

// ═══════════════════════════════════════════════════════════════════
//  PORTAL
// ═══════════════════════════════════════════════════════════════════

function Portal({ name, type, onDone }) {
  const cfg = TYPES[type];
  useEffect(() => { const t = setTimeout(onDone, 1600); return () => clearTimeout(t); }, [onDone]);

  return (
    <div style={{
      position: "fixed", inset: 0, zIndex: 200,
      background: "#0a0908",
      display: "flex", alignItems: "center", justifyContent: "center",
      animation: "portalFade 1.6s ease forwards",
    }}>
      <div style={{ textAlign: "center", animation: "portalContent 1.6s ease forwards" }}>
        <div style={{ fontSize: 36, color: "rgba(232, 168, 73, 0.6)", marginBottom: 8 }}>{cfg?.icon}</div>
        <div style={{ fontSize: 20, color: "#e8e0d4", fontFamily: "var(--serif)", fontWeight: 400 }}>{name}</div>
      </div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════════
//  MAIN
// ═══════════════════════════════════════════════════════════════════

export default function SyncVault() {
  const [entering, setEntering] = useState(null);
  const [tab, setTab] = useState("all");
  const containerRef = useRef(null);
  const [dims, setDims] = useState({ w: 900, h: 700 });

  useEffect(() => {
    function measure() {
      if (containerRef.current) setDims({ w: containerRef.current.offsetWidth, h: containerRef.current.offsetHeight });
    }
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);

  const freshCount = INCOMING.filter(a => a.fresh).length;

  const handleEnter = useCallback((id) => {
    const a = [...OUTGOING, ...INCOMING].find(x => x.id === id);
    if (a) setEntering({ name: a.name, type: a.type });
  }, []);

  if (entering) {
    return <Portal name={entering.name} type={entering.type} onDone={() => setEntering(null)} />;
  }

  return (
    <div ref={containerRef} style={{
      width: "100%", minHeight: "100vh",
      background: "#0c0b0a", color: "#e8e0d4",
      fontFamily: "var(--serif)",
      position: "relative",
    }}>
      <style>{`
        @import url('https://fonts.googleapis.com/css2?family=Source+Serif+4:opsz,wght@8..60,300;8..60,400;8..60,500&family=DM+Sans:wght@400;500;600&family=DM+Mono:wght@400&display=swap');
        :root {
          --serif: 'Source Serif 4', Georgia, serif;
          --sans: 'DM Sans', system-ui, sans-serif;
          --mono: 'DM Mono', monospace;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        @keyframes fadeIn {
          from { opacity: 0; transform: translateY(6px); }
          to { opacity: 1; transform: translateY(0); }
        }
        @keyframes syncPulse {
          0%, 100% { opacity: 0.5; }
          50% { opacity: 1; }
        }
        @keyframes portalFade {
          0% { opacity: 0; } 15% { opacity: 1; } 75% { opacity: 1; } 100% { opacity: 0; }
        }
        @keyframes portalContent {
          0% { opacity: 0; transform: scale(0.85); }
          20% { opacity: 1; transform: scale(1); }
          75% { opacity: 1; } 100% { opacity: 0; transform: scale(1.1); }
        }
        ::-webkit-scrollbar { width: 3px; }
        ::-webkit-scrollbar-track { background: transparent; }
        ::-webkit-scrollbar-thumb { background: rgba(232, 168, 73, 0.08); border-radius: 3px; }
      `}</style>

      {/* Subtle ripple from center */}
      <SyncRipple cx={dims.w / 2} cy={200} />

      {/* Header */}
      <div style={{
        padding: "24px 20px 0",
        position: "relative", zIndex: 1,
      }}>
        <div style={{
          display: "flex", justifyContent: "space-between", alignItems: "flex-end",
          marginBottom: 4,
        }}>
          <div>
            <div style={{
              fontSize: 22, fontWeight: 400, color: "#e8e0d4",
              fontFamily: "var(--serif)",
              display: "flex", alignItems: "center", gap: 10,
            }}>
              <span style={{ color: "rgba(232, 168, 73, 0.5)", fontSize: 20 }}>◈</span>
              Sync Vault
            </div>
            <div style={{
              fontSize: 12, color: "#3a3830", fontFamily: "var(--sans)",
              marginTop: 4,
            }}>
              {OUTGOING.length} outgoing · {INCOMING.length} incoming · {freshCount} fresh
            </div>
          </div>

          {/* Summary stats */}
          <div style={{
            display: "flex", gap: 16, fontSize: 10, color: "#3a3830",
            fontFamily: "var(--mono)",
          }}>
            <div style={{ textAlign: "right" }}>
              <div style={{ color: "#4a4640", marginBottom: 2 }}>synced</div>
              <div style={{ color: "#8be86b88" }}>
                {OUTGOING.reduce((s, a) => s + a.syncedWith.length, 0)}/{OUTGOING.reduce((s, a) => s + a.syncedWith.length + a.pendingSync.length, 0)}
              </div>
            </div>
            <div style={{ textAlign: "right" }}>
              <div style={{ color: "#4a4640", marginBottom: 2 }}>peers</div>
              <div>{ONLINE_PEERS.length} on · {OFFLINE_PEERS.length} off</div>
            </div>
          </div>
        </div>
      </div>

      {/* Peer status */}
      <PeerStatusBar />

      {/* Tab bar */}
      <div style={{
        display: "flex", gap: 0,
        padding: "0 20px",
        borderBottom: "1px solid rgba(232, 168, 73, 0.03)",
        position: "relative", zIndex: 1,
      }}>
        {[
          { key: "all", label: "All" },
          { key: "outgoing", label: `Outgoing (${OUTGOING.length})` },
          { key: "incoming", label: `Incoming (${INCOMING.length})`, badge: freshCount },
        ].map(t => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            style={{
              padding: "12px 16px",
              fontSize: 12, fontFamily: "var(--sans)", fontWeight: 500,
              color: tab === t.key ? "#e8a849" : "#4a4640",
              background: "none", border: "none", cursor: "pointer",
              borderBottom: tab === t.key ? "2px solid rgba(232, 168, 73, 0.4)" : "2px solid transparent",
              transition: "all 0.2s ease",
              display: "flex", alignItems: "center", gap: 6,
            }}
          >
            {t.label}
            {t.badge > 0 && (
              <span style={{
                width: 16, height: 16, borderRadius: "50%",
                background: "rgba(232, 168, 73, 0.12)",
                color: "#e8a849",
                fontSize: 10, fontFamily: "var(--mono)",
                display: "flex", alignItems: "center", justifyContent: "center",
              }}>{t.badge}</span>
            )}
          </button>
        ))}
      </div>

      {/* Content */}
      <div style={{
        padding: "16px 20px 40px",
        position: "relative", zIndex: 1,
      }}>
        {/* Outgoing section */}
        {(tab === "all" || tab === "outgoing") && (
          <div style={{ marginBottom: tab === "all" ? 28 : 0 }}>
            {tab === "all" && (
              <div style={{
                fontSize: 10, color: "#3a3830", fontFamily: "var(--sans)",
                textTransform: "uppercase", letterSpacing: "0.1em",
                marginBottom: 10, display: "flex", alignItems: "center", gap: 8,
              }}>
                <span>↑ Syncing out</span>
                <div style={{ flex: 1, height: 1, background: "rgba(232, 168, 73, 0.03)" }} />
              </div>
            )}
            <div style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))",
              gap: 8,
            }}>
              {OUTGOING.map((a, i) => (
                <OutgoingCard key={a.id} artifact={a} onEnter={handleEnter} delay={i * 0.04} />
              ))}
            </div>
          </div>
        )}

        {/* Incoming section */}
        {(tab === "all" || tab === "incoming") && (
          <div>
            {tab === "all" && (
              <div style={{
                fontSize: 10, color: "#3a3830", fontFamily: "var(--sans)",
                textTransform: "uppercase", letterSpacing: "0.1em",
                marginBottom: 10, display: "flex", alignItems: "center", gap: 8,
              }}>
                <span>↓ Synced to you</span>
                <div style={{ flex: 1, height: 1, background: "rgba(232, 168, 73, 0.03)" }} />
              </div>
            )}
            <div style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))",
              gap: 8,
            }}>
              {INCOMING.map((a, i) => (
                <IncomingCard key={a.id} artifact={a} onEnter={handleEnter} delay={i * 0.04 + (tab === "all" ? 0.2 : 0)} />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
