interface Logger {
    debug: (...args: any[]) => void;
    info: (...args: any[]) => void;
    warn: (...args: any[]) => void;
    error: (...args: any[]) => void;
}

// Simple logger with level filtering
const logLevel = (process.env.AWS_LAMBDA_LOG_LEVEL || process.env.LOG_LEVEL || 'info').toLowerCase();

const logger: Logger = {
  debug: (...args: any[]) => {
    if (logLevel === 'debug') {
      console.debug('[runtime]', ...args);
    }
  },
  info: (...args: any[]) => {
    if (logLevel === 'debug' || logLevel === 'info') {
      console.info('[runtime]', ...args);
    }
  },
  warn: (...args: any[]) => {
    if (logLevel !== 'error' && logLevel !== 'none') {
      console.warn('[runtime]', ...args);
    }
  },
  error: (...args: any[]) => {
    if (logLevel !== 'none') {
      console.error('[runtime]', ...args);
    }
  }
};

export function createLogger(prefix: string): Logger {
  return {
    debug: (...args: any[]) => logger.debug(`[${prefix}]`, ...args),
    info: (...args: any[]) => logger.info(`[${prefix}]`, ...args),
    warn: (...args: any[]) => logger.warn(`[${prefix}]`, ...args),
    error: (...args: any[]) => logger.error(`[${prefix}]`, ...args)
  };
}

export default logger; 