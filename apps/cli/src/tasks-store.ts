import * as fs from 'fs';

import { getStorageDir, getStoragePath } from './storage-context';

const CONFIG_DIR = () => getStorageDir();
export const getTasksFilePath = () => getStoragePath('tasks.json');

export interface PersistedTask {
  id: string;
  type: string;
  status: 'pending' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
  params: Record<string, unknown>;
  progress: Record<string, unknown>;
  error?: string;
}

interface TasksFile {
  version: number;
  tasks: PersistedTask[];
}

function ensureConfigDir(): void {
  if (!fs.existsSync(CONFIG_DIR())) {
    fs.mkdirSync(CONFIG_DIR(), { recursive: true });
  }
}

export function loadTasks(): PersistedTask[] {
  try {
    if (fs.existsSync(getTasksFilePath())) {
      const content = fs.readFileSync(getTasksFilePath(), 'utf-8');
      const data = JSON.parse(content) as TasksFile;
      return data.tasks || [];
    }
  } catch (error) {
    console.error('Failed to load tasks:', error);
  }
  return [];
}

export function saveTasks(tasks: PersistedTask[]): void {
  ensureConfigDir();
  const data: TasksFile = {
    version: 1,
    tasks,
  };
  fs.writeFileSync(getTasksFilePath(), JSON.stringify(data, null, 2));
}

export function upsertTask(task: PersistedTask): void {
  const tasks = loadTasks();
  const index = tasks.findIndex((t) => t.id === task.id);
  if (index >= 0) {
    tasks[index] = task;
  } else {
    tasks.push(task);
  }
  saveTasks(tasks);
}

export function removeTask(taskId: string): boolean {
  const tasks = loadTasks();
  const filtered = tasks.filter((t) => t.id !== taskId);
  if (filtered.length !== tasks.length) {
    saveTasks(filtered);
    return true;
  }
  return false;
}

export function getResumableTasks(): PersistedTask[] {
  const tasks = loadTasks();
  return tasks.filter((t) => t.status === 'running' || t.status === 'paused');
}

export function clearAllTasks(): void {
  saveTasks([]);
}
