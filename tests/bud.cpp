//Solution by Mikołaj Kołek

#include "bits/stdc++.h"
#define intin *istream_iterator<int>(cin)

using namespace std;

int main() {
	ios_base::sync_with_stdio(0);
	cin.tie(0);
	
	int n, m;
	cin >> n >> m;
	
	vector<vector<bool>> grid(n, vector<bool>(n));
	for(int i = 0; i < n; i++) {
		string row;
		cin >> row;
		
		for(int j = 0; j < n; j++)
			grid[i][j] = (row[j] == '.');
	}
	
	if(m == 1) {
		int k = 0;
		
		for(int i = 0; i < n; i++) {
			int curVertical = 0, curHorizontal = 0;
			
			for(int j = 0; j < n; j++) {
				curHorizontal = (grid[i][j] ? curHorizontal + 1 : 0);
				curVertical = (grid[j][i] ? curVertical + 1 : 0);
				
				k = max({ k, curHorizontal, curVertical });
			}
		}
		
		cout << k << "\n";
	}
	else {
		auto possible = [&] (int k) {
			pair<int, int> firstPossibleHorizontally = { 1e9, 1e9 }, firstPossibleVertically = { 1e9, 1e9 };
			int horizontalCount = 0;
			vector<vector<int>> V(n, vector<int>(n + 1));
			
			for(int i = 0; i < n; i++) {
				int curHorizontal = 0;
				
				for(int j = 0; j < n; j++) {
					curHorizontal = (grid[i][j] ? curHorizontal + 1 : 0);
					
					if(curHorizontal >= k) {
						if((firstPossibleHorizontally = min(firstPossibleHorizontally, { i, j })) <= make_pair(i, j - k))
							return true;
						
						horizontalCount++;
						V[i][j - k + 1]++; V[i][j + 1]--;
					}
				}
			}
			
			for(int i = 0; i < n; i++)
				partial_sum(V[i].begin(), V[i].end(), V[i].begin());
			
			for(int i = 0; i < n; i++) {
				int curVertical = 0, curHorizontalIntersections = 0;
				
				for(int j = 0; j < n; j++) {
					curVertical = (grid[j][i] ? curVertical + 1 : 0);
					curHorizontalIntersections += V[j][i] - (j >= k ? V[j - k][i] : 0);
					
					if(curVertical >= k)
						if((firstPossibleVertically = min(firstPossibleVertically, { i, j })) <= make_pair(i, j - k) or curHorizontalIntersections < horizontalCount)
							return true;
				}
			}
			
			return false;	
		};
		possible(6);
		
		// Przedział <a, b)
		int a = 0, b = n + 1;
		while(b - a > 1) {
			int mid = a + ((b - a) / 2);
			
			if(possible(mid))
				a = mid;
			else
				b = mid;
		}
		
		cout << a << "\n";
	}
	
	return 0;
}