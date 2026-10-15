#!/usr/bin/env node
import { main } from "../dist/esm/launcher.js";
process.exitCode = main();
