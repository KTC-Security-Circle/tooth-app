import { invoke } from '@tauri-apps/api/core'

export type PreflightCheck = {
  name: string
  passed: boolean
  message: string
}

export type PreflightResult = {
  ready: boolean
  checks: PreflightCheck[]
}

export type SessionStatus = 'running' | 'failed' | 'complete'

export type SlotStatus = 'complete' | 'needs_rescan' | 'processing'

export type ScanSessionSlot = {
  slot: number
  input_dir: string
  status: SlotStatus | null
  reason: string | null
}

export type ScanSessionManifest = {
  version: number
  session_id: string
  status: SessionStatus
  current_slot: number | null
  slots: ScanSessionSlot[]
  reason: string | null
}

export type ScanSessionResult = {
  session: ScanSessionManifest
}

export type StartScanSessionRequest = {
  session_id: string
  input_dirs: string[]
}

export type SessionRequest = {
  session_id: string
}

export type RetryScanSessionRequest = {
  session_id: string
  slot: number
}

export function preflightScan(): Promise<PreflightResult> {
  return invoke<PreflightResult>('preflight_scan')
}

export function zeroTurntable(): Promise<void> {
  return invoke<void>('zero_turntable')
}

export function moveTurntableSlot(slot: number): Promise<void> {
  return invoke<void>('move_turntable_slot', { slot })
}

export function startScanSession(
  request: StartScanSessionRequest,
): Promise<ScanSessionResult> {
  return invoke<ScanSessionResult>('start_scan_session', { request })
}

export function resumeScanSession(
  request: SessionRequest,
): Promise<ScanSessionResult> {
  return invoke<ScanSessionResult>('resume_scan_session', { request })
}

export function retryScanSession(
  request: RetryScanSessionRequest,
): Promise<ScanSessionResult> {
  return invoke<ScanSessionResult>('retry_scan_session', { request })
}

export function getScanSession(
  request: SessionRequest,
): Promise<ScanSessionResult> {
  return invoke<ScanSessionResult>('get_scan_session', { request })
}
