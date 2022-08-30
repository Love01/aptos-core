// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

import {
  VStack,
  Button,
  useColorMode,
  Box,
} from '@chakra-ui/react';
import { AddIcon } from '@chakra-ui/icons';
import React, { useMemo } from 'react';
import { useUnlockedAccounts } from 'core/hooks/useAccounts';
import { Routes } from 'core/routes';
import { Account } from 'shared/types';
import {
  switchAccountErrorToast,
  switchAccountToast,
} from 'core/components/Toast';
import { secondaryHeaderInputBgColor } from 'core/colors';
import { useNavigate } from 'react-router-dom';
import AccountView from './AccountView';

export const boxShadow = 'rgba(0, 0, 0, 0.05) 0px 4px 24px 0px';

export default function SwitchAccountBody() {
  const {
    accounts,
    switchAccount,
  } = useUnlockedAccounts();
  const { colorMode } = useColorMode();
  const navigate = useNavigate();

  const onSwitchAccount = (address: string) => {
    try {
      switchAccount(address);
      switchAccountToast(address);
      navigate(Routes.wallet.path);
    } catch {
      switchAccountErrorToast();
    }
  };

  const accountsList = useMemo(() => Object.values(accounts), [accounts]);

  const handleClickAddAccount = () => {
    navigate(Routes.addAccount.path);
  };

  return (
    <VStack spacing={2} alignItems="stretch" height="100%">
      <VStack gap={1} p={2} flex={1} overflow="scroll">
        {
        accountsList.map((account: Account) => (
          <Box px={4} width="100%">
            <AccountView
              account={account}
              showCheck
              boxShadow={boxShadow}
              onClick={onSwitchAccount}
              key={account.address}
              bgColor={{
                dark: 'gray.700',
                light: 'white',
              }}
            />
          </Box>
        ))
      }
      </VStack>
      <Box px={4} width="100%">
        <Button
          size="lg"
          width="100%"
          onClick={handleClickAddAccount}
          bgColor={secondaryHeaderInputBgColor[colorMode]}
          leftIcon={<AddIcon fontSize="xs" />}
        >
          Add Account
        </Button>
      </Box>
    </VStack>
  );
}