import { useState, useEffect } from 'react';
import {
  Box,
  TextField,
  FormControl,
  InputLabel,
  Select,
  MenuItem,
  Typography,
  Chip,
  ToggleButtonGroup,
  ToggleButton,
} from '@mui/material';

interface CronExpressionBuilderProps {
  value: string;
  onChange: (value: string) => void;
}

type PresetType = 'custom' | 'hourly' | 'daily' | 'weekdays' | 'weekly' | 'monthly';

interface CronPreset {
  label: string;
  value: string;
  description: string;
}

const PRESETS: Record<PresetType, CronPreset> = {
  custom: { label: 'Custom', value: '', description: 'Custom cron expression' },
  hourly: { label: 'Every Hour', value: '0 * * * *', description: 'At minute 0 of every hour' },
  daily: { label: 'Daily', value: '0 9 * * *', description: 'Every day at 09:00' },
  weekdays: { label: 'Weekdays', value: '0 9 * * 1-5', description: 'Monday to Friday at 09:00' },
  weekly: { label: 'Weekly', value: '0 9 * * 0', description: 'Every Sunday at 09:00' },
  monthly: { label: 'Monthly', value: '0 9 1 * *', description: 'First day of every month at 09:00' },
};

const HOURS = Array.from({ length: 24 }, (_, i) => i);
const MINUTES = [0, 15, 30, 45];
const DAYS_OF_WEEK = [
  { value: '0', label: 'Sunday' },
  { value: '1', label: 'Monday' },
  { value: '2', label: 'Tuesday' },
  { value: '3', label: 'Wednesday' },
  { value: '4', label: 'Thursday' },
  { value: '5', label: 'Friday' },
  { value: '6', label: 'Saturday' },
];

export default function CronExpressionBuilder({ value, onChange }: CronExpressionBuilderProps) {
  const [preset, setPreset] = useState<PresetType>('daily');
  const [customMode, setCustomMode] = useState(false);
  const [minute, setMinute] = useState(0);
  const [hour, setHour] = useState(9);
  const [selectedDays, setSelectedDays] = useState<string[]>([]);
  const [customCron, setCustomCron] = useState(value);

  useEffect(() => {
    // Detect which preset matches the current value
    const matchingPreset = Object.entries(PRESETS).find(
      ([key, preset]) => key !== 'custom' && preset.value === value
    );

    if (matchingPreset) {
      setPreset(matchingPreset[0] as PresetType);
      setCustomMode(false);
    } else {
      setPreset('custom');
      setCustomMode(true);
      setCustomCron(value);
    }

    // Parse cron for custom builder
    const parts = value.split(' ');
    if (parts.length >= 5) {
      const [m, h, , , dow] = parts;
      if (!isNaN(Number(m))) setMinute(Number(m));
      if (!isNaN(Number(h))) setHour(Number(h));
      if (dow !== '*') {
        const days = dow.split(',');
        setSelectedDays(days);
      }
    }
  }, [value]);

  const handlePresetChange = (newPreset: PresetType) => {
    setPreset(newPreset);
    if (newPreset === 'custom') {
      setCustomMode(true);
    } else {
      setCustomMode(false);
      onChange(PRESETS[newPreset].value);
    }
  };

  const buildCronExpression = (): string => {
    if (customMode) {
      return customCron;
    }

    let cron = `${minute} ${hour} * * `;

    if (selectedDays.length > 0) {
      cron += selectedDays.sort().join(',');
    } else {
      cron += '*';
    }

    return cron;
  };

  const handleTimeChange = () => {
    const newCron = buildCronExpression();
    onChange(newCron);
  };


  const handleCustomCronChange = (newValue: string) => {
    setCustomCron(newValue);
    onChange(newValue);
  };

  return (
    <Box>
      {/* Preset Selector */}
      <FormControl fullWidth size="small" sx={{ mb: 2 }}>
        <InputLabel>Preset</InputLabel>
        <Select value={preset} onChange={(e) => handlePresetChange(e.target.value as PresetType)} label="Preset">
          {Object.entries(PRESETS).map(([key, preset]) => (
            <MenuItem key={key} value={key}>
              {preset.label}
            </MenuItem>
          ))}
        </Select>
      </FormControl>

      {customMode ? (
        // Custom Cron Expression Input
        <Box>
          <TextField
            fullWidth
            label="Cron Expression"
            value={customCron}
            onChange={(e) => handleCustomCronChange(e.target.value)}
            size="small"
            helperText="Format: minute hour day month day-of-week (e.g., 0 9 * * * = daily at 09:00)"
          />
          <Typography variant="caption" color="text.secondary" display="block" mt={1}>
            Examples:
            <br />• 0 9 * * * - Every day at 09:00
            <br />• 0 */2 * * * - Every 2 hours
            <br />• 30 14 * * 1-5 - Weekdays at 14:30
            <br />• 0 0 1 * * - First day of month at 00:00
          </Typography>
        </Box>
      ) : (
        // Simple Builder
        <Box>
          <Box display="flex" gap={2} mb={2}>
            <FormControl fullWidth size="small">
              <InputLabel>Hour</InputLabel>
              <Select
                value={hour}
                onChange={(e) => {
                  setHour(e.target.value as number);
                  setTimeout(handleTimeChange, 0);
                }}
                label="Hour"
              >
                {HOURS.map((h) => (
                  <MenuItem key={h} value={h}>
                    {h.toString().padStart(2, '0')}:00
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
            <FormControl fullWidth size="small">
              <InputLabel>Minute</InputLabel>
              <Select
                value={minute}
                onChange={(e) => {
                  setMinute(e.target.value as number);
                  setTimeout(handleTimeChange, 0);
                }}
                label="Minute"
              >
                {MINUTES.map((m) => (
                  <MenuItem key={m} value={m}>
                    :{m.toString().padStart(2, '0')}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Box>

          {preset !== 'hourly' && (
            <Box>
              <Typography variant="subtitle2" gutterBottom>
                Days of Week (optional)
              </Typography>
              <ToggleButtonGroup
                value={selectedDays}
                onChange={(_, newDays) => {
                  setSelectedDays(newDays);
                  const newCron = `${minute} ${hour} * * ${
                    newDays.length > 0 ? newDays.sort().join(',') : '*'
                  }`;
                  onChange(newCron);
                }}
                aria-label="days of week"
                size="small"
                sx={{ flexWrap: 'wrap', gap: 0.5 }}
              >
                {DAYS_OF_WEEK.map((day) => (
                  <ToggleButton
                    key={day.value}
                    value={day.value}
                    aria-label={day.label}
                    sx={{ px: 1, py: 0.5 }}
                  >
                    {day.label.slice(0, 3)}
                  </ToggleButton>
                ))}
              </ToggleButtonGroup>
              <Typography variant="caption" color="text.secondary" display="block" mt={1}>
                Leave empty for every day
              </Typography>
            </Box>
          )}

          <Box mt={2}>
            <Chip
              label={`Cron: ${buildCronExpression()}`}
              size="small"
              variant="outlined"
              color="primary"
            />
            <Typography variant="caption" color="text.secondary" display="block" mt={1}>
              {PRESETS[preset].description}
            </Typography>
          </Box>
        </Box>
      )}
    </Box>
  );
}
