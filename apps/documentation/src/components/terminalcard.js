import * as React from 'react';
import Box from '@mui/material/Box';
import Card from '@mui/material/Card';
import CardActions from '@mui/material/CardActions';
import Button from '@mui/material/Button';
import styles from './HomepageFeatures.module.css';
import Grid from '@mui/material/Grid';

const bull = (
  <Box component="span" sx={{ display: 'inline-block', mx: '2px', transform: 'scale(0.8)' }}>
    •
  </Box>
);

export default function BasicCard() {
  return (
    <a href="https://v3.tealstreet.io/trade/" target="_blank">
      <div className={styles.tradebutton}>
        <div className={styles.tradebuttoncard}>
          <Card sx={{ minWidth: 200 }}>
            <CardActions>
              <Button size="Large">
                <div>Start Trading</div>
              </Button>
            </CardActions>
          </Card>
        </div>
      </div>
    </a>
  );
}
