import { useEffect, useState } from 'react';
import {
  Box,
  Typography,
  Button,
  Card,
  CardContent,
  IconButton,
  Switch,
  Chip,
  Alert,
  CircularProgress,
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  TextField,
  FormControl,
  InputLabel,
  Select,
  MenuItem,
  FormControlLabel,
  Checkbox,
} from '@mui/material';
import DeleteIcon from '@mui/icons-material/Delete';
import EditIcon from '@mui/icons-material/Edit';
import AddIcon from '@mui/icons-material/Add';
import ScheduleIcon from '@mui/icons-material/Schedule';
import {
  getRecordingSchedules,
  addRecordingSchedule,
  updateRecordingSchedule,
  deleteRecordingSchedule,
  toggleSchedule,
  getCameras,
  getRecordingCameras,
  stopRecording,
  type RecordingSchedule,
  type Camera,
  type NewRecordingSchedule,
  type UpdateRecordingSchedule,
} from '../services/api';
import CronExpressionBuilder from './CronExpressionBuilder';

interface ScheduleRecordingProps {
  onScheduleChanged?: () => void;
}

export default function ScheduleRecording({ onScheduleChanged }: ScheduleRecordingProps) {
  const [schedules, setSchedules] = useState<RecordingSchedule[]>([]);
  const [cameras, setCameras] = useState<Camera[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [editingSchedule, setEditingSchedule] = useState<RecordingSchedule | null>(null);
  const [recordingCameraIds, setRecordingCameraIds] = useState<number[]>([]);

  // Form state
  const [selectedCameraId, setSelectedCameraId] = useState<number | ''>('');
  const [scheduleName, setScheduleName] = useState('');
  const [cronExpression, setCronExpression] = useState('0 9 * * *');
  const [durationMinutes, setDurationMinutes] = useState(30);
  const [fps, setFps] = useState<number | ''>('');
  const [isEnabled, setIsEnabled] = useState(true);
  const [formError, setFormError] = useState<string | null>(null);

  useEffect(() => {
    loadData();

    // Poll for recording status every 3 seconds
    const interval = setInterval(async () => {
      try {
        const cameraIds = await getRecordingCameras();
        setRecordingCameraIds(cameraIds);
      } catch (err) {
        console.error('Failed to get recording cameras:', err);
      }
    }, 3000);

    return () => clearInterval(interval);
  }, []);

  const loadData = async () => {
    setLoading(true);
    setError(null);
    try {
      const [schedulesData, camerasData] = await Promise.all([
        getRecordingSchedules(),
        getCameras(),
      ]);
      setSchedules(schedulesData);
      setCameras(camerasData);
    } catch (err: any) {
      setError(`Failed to load data: ${err}`);
      console.error('Failed to load schedules:', err);
    } finally {
      setLoading(false);
    }
  };

  const handleOpenDialog = (schedule?: RecordingSchedule) => {
    if (schedule) {
      setEditingSchedule(schedule);
      setSelectedCameraId(schedule.camera_id);
      setScheduleName(schedule.name);
      setCronExpression(schedule.cron_expression);
      setDurationMinutes(schedule.duration_minutes);
      setFps(schedule.fps ?? '');
      setIsEnabled(schedule.is_enabled);
    } else {
      resetForm();
    }
    setIsDialogOpen(true);
  };

  const resetForm = () => {
    setEditingSchedule(null);
    setSelectedCameraId('');
    setScheduleName('');
    setCronExpression('0 9 * * *');
    setDurationMinutes(30);
    setFps('');
    setIsEnabled(true);
    setFormError(null);
  };

  const handleCloseDialog = () => {
    setIsDialogOpen(false);
    resetForm();
  };

  const validateForm = (): boolean => {
    if (!selectedCameraId) {
      setFormError('Please select a camera');
      return false;
    }
    if (!scheduleName.trim()) {
      setFormError('Please enter a schedule name');
      return false;
    }
    if (!cronExpression.trim()) {
      setFormError('Please enter a cron expression');
      return false;
    }
    if (durationMinutes <= 0) {
      setFormError('Duration must be greater than 0');
      return false;
    }
    if (fps !== '' && (typeof fps !== 'number' || fps <= 0)) {
      setFormError('FPS must be a positive number');
      return false;
    }
    setFormError(null);
    return true;
  };

  const handleSave = async () => {
    if (!validateForm()) return;

    try {
      const scheduleData: NewRecordingSchedule = {
        camera_id: selectedCameraId as number,
        name: scheduleName.trim(),
        cron_expression: cronExpression.trim(),
        duration_minutes: durationMinutes,
        fps: fps === '' ? null : (fps as number),
        is_enabled: isEnabled,
      };

      if (editingSchedule) {
        const updates: UpdateRecordingSchedule = {
          name: scheduleName.trim(),
          cron_expression: cronExpression.trim(),
          duration_minutes: durationMinutes,
          fps: fps === '' ? null : (fps as number),
          is_enabled: isEnabled,
        };
        await updateRecordingSchedule(editingSchedule.id, updates);
      } else {
        await addRecordingSchedule(scheduleData);
      }

      handleCloseDialog();
      await loadData();
      onScheduleChanged?.();
    } catch (err: any) {
      setFormError(`Failed to save schedule: ${err}`);
      console.error('Failed to save schedule:', err);
    }
  };

  const handleDelete = async (id: number, name: string) => {
    if (!window.confirm(`Are you sure you want to delete schedule "${name}"?`)) {
      return;
    }

    try {
      await deleteRecordingSchedule(id);
      await loadData();
      onScheduleChanged?.();
    } catch (err: any) {
      console.error('Failed to delete schedule:', err);
      setError(`Failed to delete schedule: ${err}`);
    }
  };

  const handleToggle = async (id: number, currentEnabled: boolean) => {
    try {
      await toggleSchedule(id, !currentEnabled);
      await loadData();
      onScheduleChanged?.();
    } catch (err: any) {
      console.error('Failed to toggle schedule:', err);
      setError(`Failed to toggle schedule: ${err}`);
    }
  };

  const handleStopRecording = async (cameraId: number, scheduleName: string) => {
    if (!window.confirm(`Stop recording for "${scheduleName}"?`)) {
      return;
    }

    try {
      await stopRecording(cameraId);
      // Update recording status immediately
      setRecordingCameraIds(prev => prev.filter(id => id !== cameraId));
    } catch (err: any) {
      console.error('Failed to stop recording:', err);
      setError(`Failed to stop recording: ${err}`);
    }
  };

  const getCameraName = (cameraId: number): string => {
    const camera = cameras.find((c) => c.id === cameraId);
    return camera?.name ?? 'Unknown Camera';
  };

  const formatCronDescription = (cron: string): string => {
    // Simple cron description (can be enhanced)
    const parts = cron.split(' ');
    if (parts.length < 5) return cron;

    const [minute, hour, day, month, dayOfWeek] = parts;

    if (minute === '0' && hour === '9' && day === '*' && month === '*' && dayOfWeek === '*') {
      return 'Daily at 09:00';
    }
    if (minute === '0' && hour.startsWith('*/') && day === '*' && month === '*' && dayOfWeek === '*') {
      return `Every ${hour.slice(2)} hours`;
    }
    if (minute === '0' && hour === '9' && day === '*' && month === '*' && dayOfWeek === '1-5') {
      return 'Weekdays at 09:00';
    }

    return cron;
  };

  if (loading) {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" minHeight={200}>
        <CircularProgress />
      </Box>
    );
  }

  return (
    <Box sx={{ mt: 4 }}>
      <Box display="flex" justifyContent="space-between" alignItems="center" mb={2}>
        <Typography variant="h4" component="h2" className="text-gray-700 font-medium">
          <ScheduleIcon sx={{ mr: 1, verticalAlign: 'middle' }} />
          Recording Schedules
        </Typography>
        <Button
          variant="contained"
          startIcon={<AddIcon />}
          onClick={() => handleOpenDialog()}
          className="bg-blue-600"
        >
          Add Schedule
        </Button>
      </Box>

      {error && (
        <Alert severity="error" sx={{ mb: 2 }}>
          {error}
        </Alert>
      )}

      {schedules.length === 0 ? (
        <Alert severity="info">
          No recording schedules configured. Click "Add Schedule" to create one.
        </Alert>
      ) : (
        <Box
          sx={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(350px, 1fr))',
            gap: 2,
          }}
        >
          {schedules.map((schedule) => {
            const isRecording = recordingCameraIds.includes(schedule.camera_id);

            return (
              <Card key={schedule.id} className="shadow-md">
                <CardContent>
                  <Box display="flex" justifyContent="space-between" alignItems="start" mb={2}>
                    <Box>
                      <Typography variant="h6" component="div" gutterBottom>
                        {schedule.name}
                      </Typography>
                      <Box display="flex" gap={1} alignItems="center" flexWrap="wrap">
                        <Chip
                          label={schedule.camera_name || getCameraName(schedule.camera_id)}
                          size="small"
                          color="primary"
                          variant="outlined"
                        />
                        {/* Schedule Status with Next Run Time */}
                        <Chip
                          label={
                            schedule.next_run
                              ? `Next: ${new Date(schedule.next_run).toLocaleString('ja-JP', {
                                  timeZone: 'Asia/Tokyo',
                                  year: 'numeric',
                                  month: '2-digit',
                                  day: '2-digit',
                                  hour: '2-digit',
                                  minute: '2-digit',
                                })}`
                              : "Inactive"
                          }
                          size="small"
                          color={schedule.next_run ? "success" : "default"}
                          variant={schedule.next_run ? "filled" : "outlined"}
                        />
                        {/* Recording Status */}
                        {isRecording && (
                          <Chip
                            label="Recording"
                            size="small"
                            color="error"
                            icon={<span className="animate-pulse">●</span>}
                          />
                        )}
                      </Box>
                    </Box>
                    <FormControlLabel
                      control={
                        <Switch
                          checked={schedule.is_enabled}
                          onChange={() => handleToggle(schedule.id, schedule.is_enabled)}
                          color="primary"
                        />
                      }
                      label=""
                    />
                  </Box>

                <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                  <strong>Schedule:</strong> {formatCronDescription(schedule.cron_expression)}
                </Typography>

                <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                  <strong>Duration:</strong> {schedule.duration_minutes} minutes
                </Typography>

                {schedule.fps && (
                  <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                    <strong>FPS:</strong> {schedule.fps}
                  </Typography>
                )}

                <Box display="flex" justifyContent="space-between" alignItems="center" mt={2}>
                  {isRecording && (
                    <Button
                      size="small"
                      variant="contained"
                      color="error"
                      onClick={() => handleStopRecording(schedule.camera_id, schedule.name)}
                    >
                      Stop Recording
                    </Button>
                  )}
                  <Box display="flex" gap={1} ml="auto">
                    <IconButton
                      size="small"
                      onClick={() => handleOpenDialog(schedule)}
                      title="Edit schedule"
                    >
                      <EditIcon fontSize="small" />
                    </IconButton>
                    <IconButton
                      size="small"
                      color="error"
                      onClick={() => handleDelete(schedule.id, schedule.name)}
                      title="Delete schedule"
                    >
                      <DeleteIcon fontSize="small" />
                    </IconButton>
                  </Box>
                </Box>
              </CardContent>
            </Card>
            );
          })}
        </Box>
      )}

      {/* Add/Edit Schedule Dialog */}
      <Dialog open={isDialogOpen} onClose={handleCloseDialog} maxWidth="sm" fullWidth>
        <DialogTitle>
          {editingSchedule ? 'Edit Recording Schedule' : 'Add Recording Schedule'}
        </DialogTitle>
        <DialogContent>
          {formError && (
            <Alert severity="error" sx={{ mb: 2 }}>
              {formError}
            </Alert>
          )}

          <FormControl fullWidth sx={{ mt: 2 }}>
            <InputLabel>Camera</InputLabel>
            <Select
              value={selectedCameraId}
              onChange={(e) => setSelectedCameraId(e.target.value as number)}
              label="Camera"
              disabled={!!editingSchedule}
            >
              {cameras.map((camera) => (
                <MenuItem key={camera.id} value={camera.id}>
                  {camera.name} ({camera.type})
                </MenuItem>
              ))}
            </Select>
          </FormControl>

          <TextField
            fullWidth
            label="Schedule Name"
            value={scheduleName}
            onChange={(e) => setScheduleName(e.target.value)}
            margin="normal"
            placeholder="e.g., Daily Morning Recording"
          />

          <Box mt={2}>
            <Typography variant="subtitle2" gutterBottom>
              Schedule (Cron Expression)
            </Typography>
            <CronExpressionBuilder
              value={cronExpression}
              onChange={setCronExpression}
            />
          </Box>

          <TextField
            fullWidth
            type="number"
            label="Duration (minutes)"
            value={durationMinutes}
            onChange={(e) => setDurationMinutes(parseInt(e.target.value) || 0)}
            margin="normal"
            inputProps={{ min: 1 }}
          />

          <TextField
            fullWidth
            type="number"
            label="FPS (optional)"
            value={fps}
            onChange={(e) => setFps(e.target.value ? parseInt(e.target.value) : '')}
            margin="normal"
            inputProps={{ min: 1 }}
            placeholder="Leave empty for camera default"
          />

          <FormControlLabel
            control={
              <Checkbox
                checked={isEnabled}
                onChange={(e) => setIsEnabled(e.target.checked)}
              />
            }
            label="Enable schedule"
            sx={{ mt: 1 }}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={handleCloseDialog}>Cancel</Button>
          <Button onClick={handleSave} variant="contained" color="primary">
            {editingSchedule ? 'Update' : 'Add'}
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
}
