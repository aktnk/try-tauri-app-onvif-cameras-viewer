import React, { useEffect, useState } from 'react';
import {
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  Button,
  FormControl,
  InputLabel,
  Select,
  MenuItem,
  TextField,
  Alert,
  CircularProgress,
  Box,
  Typography,
  Chip,
} from '@mui/material';
import {
  detectGpu,
  getEncoderSettings,
  updateEncoderSettings,
  GpuCapabilities,
  EncoderSettings as EncoderSettingsType,
} from '../services/api';

interface EncoderSettingsProps {
  open: boolean;
  onClose: () => void;
}

export default function EncoderSettings({ open, onClose }: EncoderSettingsProps) {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [capabilities, setCapabilities] = useState<GpuCapabilities | null>(null);
  const [settings, setSettings] = useState<EncoderSettingsType | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      loadData();
    }
  }, [open]);

  const loadData = async () => {
    setLoading(true);
    setError(null);
    try {
      const [caps, sets] = await Promise.all([
        detectGpu(),
        getEncoderSettings(),
      ]);
      setCapabilities(caps);
      setSettings(sets);
    } catch (err: any) {
      setError(`Failed to load settings: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    if (!settings) return;

    setSaving(true);
    setError(null);
    setSuccessMessage(null);

    try {
      const updated = await updateEncoderSettings({
        encoderMode: settings.encoderMode,
        gpuEncoder: settings.gpuEncoder,
        cpuEncoder: settings.cpuEncoder,
        preset: settings.preset,
        quality: settings.quality,
      });
      setSettings(updated);
      setSuccessMessage('設定を保存しました');

      // 保存成功後、1秒待ってからダイアログを閉じる
      setTimeout(() => {
        handleClose();
      }, 1000);
    } catch (err: any) {
      setError(`Failed to save settings: ${err}`);
    } finally {
      setSaving(false);
    }
  };

  const handleClose = () => {
    setSuccessMessage(null);
    setError(null);
    onClose();
  };

  if (loading) {
    return (
      <Dialog open={open} onClose={handleClose}>
        <DialogContent>
          <Box display="flex" justifyContent="center" alignItems="center" minHeight={200}>
            <CircularProgress />
          </Box>
        </DialogContent>
      </Dialog>
    );
  }

  return (
    <Dialog open={open} onClose={handleClose} maxWidth="sm" fullWidth>
      <DialogTitle>エンコーダー設定</DialogTitle>
      <DialogContent>
        {error && (
          <Alert severity="error" sx={{ mb: 2 }}>
            {error}
          </Alert>
        )}
        {successMessage && (
          <Alert severity="success" sx={{ mb: 2 }}>
            {successMessage}
          </Alert>
        )}

        {/* GPU情報表示 */}
        <Box mb={3}>
          <Typography variant="subtitle2" gutterBottom>
            GPU情報
          </Typography>
          <Box display="flex" gap={1} alignItems="center">
            <Chip
              label={capabilities?.gpuType || 'なし'}
              color={capabilities?.gpuType !== 'None' ? 'primary' : 'default'}
              size="small"
            />
            {capabilities?.gpuName && (
              <Typography variant="body2" color="text.secondary">
                {capabilities.gpuName}
              </Typography>
            )}
          </Box>
          {capabilities?.availableEncoders && capabilities.availableEncoders.length > 0 && (
            <Box mt={1}>
              <Typography variant="caption" color="text.secondary">
                利用可能なエンコーダー: {capabilities.availableEncoders.join(', ')}
              </Typography>
            </Box>
          )}
        </Box>

        {settings && (
          <>
            {/* エンコーダーモード選択 */}
            <FormControl fullWidth margin="normal">
              <InputLabel>エンコーダーモード</InputLabel>
              <Select
                value={settings.encoderMode}
                label="エンコーダーモード"
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    encoderMode: e.target.value as 'Auto' | 'GpuOnly' | 'CpuOnly',
                  })
                }
              >
                <MenuItem value="Auto">自動 (GPU優先、フォールバックあり)</MenuItem>
                <MenuItem
                  value="GpuOnly"
                  disabled={!capabilities?.preferredEncoder}
                >
                  GPUのみ
                  {!capabilities?.preferredEncoder && ' (利用不可)'}
                </MenuItem>
                <MenuItem value="CpuOnly">CPUのみ</MenuItem>
              </Select>
            </FormControl>

            {/* GPU選択 */}
            {capabilities?.availableEncoders && capabilities.availableEncoders.length > 0 && (
              <FormControl fullWidth margin="normal">
                <InputLabel>GPUエンコーダー</InputLabel>
                <Select
                  value={settings.gpuEncoder || ''}
                  label="GPUエンコーダー"
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      gpuEncoder: e.target.value || null,
                    })
                  }
                >
                  {capabilities.availableEncoders.map((enc) => (
                    <MenuItem key={enc} value={enc}>
                      {enc}
                      {enc === capabilities.preferredEncoder && ' (推奨)'}
                    </MenuItem>
                  ))}
                </Select>
              </FormControl>
            )}

            {/* CPUエンコーダー */}
            <FormControl fullWidth margin="normal">
              <InputLabel>CPUエンコーダー (フォールバック)</InputLabel>
              <Select
                value={settings.cpuEncoder}
                label="CPUエンコーダー (フォールバック)"
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    cpuEncoder: e.target.value,
                  })
                }
              >
                <MenuItem value="libx264">libx264 (H.264)</MenuItem>
                <MenuItem value="libx265">libx265 (H.265/HEVC)</MenuItem>
              </Select>
            </FormControl>

            {/* プリセット */}
            <FormControl fullWidth margin="normal">
              <InputLabel>エンコードプリセット</InputLabel>
              <Select
                value={settings.preset}
                label="エンコードプリセット"
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    preset: e.target.value,
                  })
                }
              >
                <MenuItem value="ultrafast">ultrafast (最速、品質低)</MenuItem>
                <MenuItem value="superfast">superfast</MenuItem>
                <MenuItem value="veryfast">veryfast</MenuItem>
                <MenuItem value="faster">faster</MenuItem>
                <MenuItem value="fast">fast (推奨)</MenuItem>
                <MenuItem value="medium">medium (バランス)</MenuItem>
              </Select>
            </FormControl>

            {/* 品質 */}
            <TextField
              fullWidth
              margin="normal"
              label="品質 (CRF/CQ)"
              type="number"
              value={settings.quality}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  quality: parseInt(e.target.value, 10),
                })
              }
              inputProps={{
                min: 18,
                max: 28,
                step: 1,
              }}
              helperText="18-28 (低いほど高品質、推奨: 23)"
            />
          </>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={handleClose} disabled={saving}>
          キャンセル
        </Button>
        <Button onClick={handleSave} variant="contained" disabled={saving || !settings}>
          {saving ? <CircularProgress size={24} /> : '保存'}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
