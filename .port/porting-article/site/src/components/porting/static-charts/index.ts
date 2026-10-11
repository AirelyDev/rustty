import type { ComponentType } from "react";
import AuditFunnel from "./AuditFunnel";
import ChecksGrowth from "./ChecksGrowth";
import Orchestration from "./Orchestration";
import PlatformTable from "./PlatformTable";
import StatsCard from "./StatsCard";
import StrategyTimeline from "./StrategyTimeline";
import UnsafeDensity from "./UnsafeDensity";

/**
 * STATIC_CHARTS maps each static chart's slug to its server component.
 * PortingWidget renders these directly inside the card (no lazy client
 * mount); their only client code is the shared Tooltip.
 */
export const STATIC_CHARTS: Record<string, ComponentType> = {
  "stats-card": StatsCard,
  "strategy-timeline": StrategyTimeline,
  "checks-growth": ChecksGrowth,
  "audit-funnel": AuditFunnel,
  orchestration: Orchestration,
  "unsafe-density": UnsafeDensity,
  "platform-table": PlatformTable,
};
