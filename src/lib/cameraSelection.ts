type CameraSelection =
  | { error: string }
  | { leftCameraId: number; rightCameraId: number }

export function parseCameraId(path: string): number | null {
  const numeric = path.replace(/^\/dev\/video/, '').replace(/^\+/, '')
  if (!numeric || /\D/.test(numeric)) return null
  const id = Number(numeric)
  return Number.isInteger(id) && id <= 2_147_483_647 ? id : null
}

export function resolveCameraSelection(
  cameraLeft: string,
  cameraRight: string,
): CameraSelection {
  if (!cameraLeft) return { error: '設定で左カメラを選択してください。' }
  if (!cameraRight) return { error: '設定で右カメラを選択してください。' }
  const leftCameraId = parseCameraId(cameraLeft)
  const rightCameraId = parseCameraId(cameraRight)
  if (leftCameraId === null || rightCameraId === null) {
    return {
      error:
        'カメラには /dev/videoN または数値のカメラ ID を指定してください。',
    }
  }
  if (leftCameraId === rightCameraId) {
    return { error: '左右に別々のカメラを選択してください。' }
  }
  return { leftCameraId, rightCameraId }
}
