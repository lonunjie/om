import type { GroupProtectionLevel, GroupProtectionView, GroupRoomSetting } from "@om/platform-api";

/**
 * Display-only mirror of the Room defaults per protection level (ADR-0189 section 3), used to name what a choice changes before
 * it is made. Core owns the bundles and refuses what it does not accept; this table only words the preview and decides whether to
 * ask first. A mismatch can make the screen ask for a confirmation it did not need or show a line that is too short (Core still
 * refuses an unconfirmed loosening), never loosen a Room.
 */
export interface RoomFields {
  readonly profile: GroupProtectionLevel;
  /** 1 invite_only (automatic once they tap Join), 2 admin_approval. Any other value is `open`. */
  readonly joinRule: number;
  /** 1 automatic, 2 admin_approval. */
  readonly newDevicePolicy: number;
  readonly historyPolicy: number;
  readonly membersMayInvite: boolean;
}

export const PROTECTION_LEVELS: readonly GroupProtectionLevel[] = [1, 2, 3];

const BUNDLES: Readonly<Record<GroupProtectionLevel, RoomFields>> = {
  1: { profile: 1, joinRule: 1, newDevicePolicy: 1, historyPolicy: 1, membersMayInvite: true },
  2: { profile: 2, joinRule: 2, newDevicePolicy: 1, historyPolicy: 1, membersMayInvite: false },
  3: { profile: 3, joinRule: 2, newDevicePolicy: 2, historyPolicy: 1, membersMayInvite: false },
};
export const bundleOf = (level: GroupProtectionLevel): RoomFields => BUNDLES[level];

/** What a change makes less strict; each key has a plain line in the copy. */
export type LoosenKey = "add" | "join" | "device" | "history" | "level";

const joinRank = (v: number): number => (v === 1 ? 1 : v === 2 ? 2 : 0);
const historyRank = (v: number): number => (v === 1 ? 2 : v === 2 ? 1 : 0);

export function loosenedBy(cur: GroupProtectionView, next: RoomFields): readonly LoosenKey[] {
  const out: LoosenKey[] = [];
  if (!cur.membersMayInvite && next.membersMayInvite) out.push("add");
  if (joinRank(next.joinRule) < joinRank(cur.joinRule)) out.push("join");
  if (cur.newDevicePolicy === 2 && next.newDevicePolicy !== 2) out.push("device");
  if (historyRank(next.historyPolicy) < historyRank(cur.historyPolicy)) out.push("history");
  if (next.profile < cur.profile) out.push("level");
  return out;
}

export const asLevel = (n: number): GroupProtectionLevel | null => (n === 1 || n === 2 || n === 3 ? n : null);

/** The fields as they would be after one setting (a level moves to its whole bundle). `null`: not something a screen offers. */
export function afterSetting(cur: GroupProtectionView, s: GroupRoomSetting): RoomFields | null {
  const base: RoomFields = { profile: asLevel(cur.profile) ?? 1, joinRule: cur.joinRule, newDevicePolicy: cur.newDevicePolicy, historyPolicy: cur.historyPolicy, membersMayInvite: cur.membersMayInvite };
  if ("protection" in s) return bundleOf(s.protection);
  if ("join_rule" in s) return { ...base, joinRule: s.join_rule };
  if ("new_device_policy" in s) return { ...base, newDevicePolicy: s.new_device_policy };
  if ("members_may_invite" in s) return { ...base, membersMayInvite: s.members_may_invite };
  return null;
}
