// BeamNG. Drive Multiplayer Server.cpp : This file contains the 'main' function. Program execution begins and ends there.
//

#include "pch.h"
#include <iostream>
#include <string.h>

using namespace std;
int main(int argc, char *argv[], char *envp[]) {
	int iNumberLines = 0;    // Default is no line numbers.

	// If /n is passed to the .exe, display numbered listing
	// of environment variables.

	if ((argc == 2) && _stricmp(argv[1], "/n") == 0)
		iNumberLines = 1;

	// Walk through list of strings until a NULL is encountered.
	for (int i = 0; envp[i] != NULL; ++i) {
		if (iNumberLines)
			cout << i << ": " << envp[i] << "\n";
	}
}