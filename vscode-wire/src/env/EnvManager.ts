import { writeFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import {
  parseEnvironmentFile,
  parseCollectionFile,
  serializeEnvironment,
} from '../core/yaml.js';
import type { Environment } from '../core/types.js';

export interface WireEnvironment extends Environment {
  file: string;
}

/**
 * Reads and writes Wire environment files.
 */
export class EnvManager {
  constructor(private wireDir: string) {}

  /** List all available environments */
  async listEnvironments(): Promise<WireEnvironment[]> {
    const envsDir = join(this.wireDir, 'envs');
    const envs: WireEnvironment[] = [];

    try {
      const files = await readdir(envsDir);

      for (const file of files) {
        if (!file.endsWith('.yaml')) continue;
        const env = await this.loadEnvironment(file);
        if (env) envs.push(env);
      }
    } catch {
      // envs/ directory doesn't exist
    }

    return envs;
  }

  /** Load a single environment file */
  async loadEnvironment(fileName: string): Promise<WireEnvironment | null> {
    try {
      const filePath = join(this.wireDir, 'envs', fileName);
      const env = await parseEnvironmentFile(filePath);
      return { ...env, file: fileName };
    } catch {
      return null;
    }
  }

  /** Save an environment file */
  async saveEnvironment(env: WireEnvironment): Promise<void> {
    const filePath = join(this.wireDir, 'envs', env.file);
    await writeFile(filePath, serializeEnvironment(env));
  }

  /** Get the active environment name from wire.yaml */
  async getActiveEnv(): Promise<string> {
    try {
      const config = await parseCollectionFile(join(this.wireDir, 'wire.yaml'));
      return config.active_env ?? 'dev';
    } catch {
      return 'dev';
    }
  }
}
