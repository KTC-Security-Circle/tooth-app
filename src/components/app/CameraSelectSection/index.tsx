import SectionPanel from '@/components/app/SectionPanel'
import FormField from '@/components/ui/FormField'
import type { CameraInfo } from '@/hooks/useSettings'

interface Props {
  cameras: CameraInfo[]
  cameraLeft: string
  cameraRight: string
  onCameraLeftChange: (value: string) => void
  onCameraRightChange: (value: string) => void
}

const CameraSelectSection: React.FC<Props> = ({
  cameras,
  cameraLeft,
  cameraRight,
  onCameraLeftChange,
  onCameraRightChange,
}) => {
  return (
    <SectionPanel heading="カメラ選択">
      <div className="space-y-4">
        <FormField>
          <FormField.Label htmlFor="camera-left">左</FormField.Label>
          <FormField.Select
            id="camera-left"
            value={cameraLeft}
            onChange={(e) => {
              onCameraLeftChange(e.currentTarget.value)
            }}
          >
            <FormField.Select.Option value="">未選択</FormField.Select.Option>
            {cameras.map((camera) => (
              <FormField.Select.Option key={camera.path} value={camera.path}>
                {camera.name} ({camera.path})
              </FormField.Select.Option>
            ))}
          </FormField.Select>
        </FormField>

        <FormField>
          <FormField.Label htmlFor="camera-right">右</FormField.Label>
          <FormField.Select
            id="camera-right"
            value={cameraRight}
            onChange={(e) => {
              onCameraRightChange(e.currentTarget.value)
            }}
          >
            <FormField.Select.Option value="">未選択</FormField.Select.Option>
            {cameras.map((camera) => (
              <FormField.Select.Option key={camera.path} value={camera.path}>
                {camera.name} ({camera.path})
              </FormField.Select.Option>
            ))}
          </FormField.Select>
        </FormField>
      </div>
    </SectionPanel>
  )
}

export default CameraSelectSection
