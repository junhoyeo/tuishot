import chalk from "chalk";
import React from "react";
import { Box, Text, renderToString } from "ink";

export function renderExampleToAnsi() {
  chalk.level = 3;
  return renderToString(
    <Box borderStyle="round" paddingX={1}>
      <Text bold color="green">tuishot</Text>
      <Text> + Ink</Text>
    </Box>,
    { columns: 24 },
  );
}
