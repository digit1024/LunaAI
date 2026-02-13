---
name: gh-project-management
description: GitHub CLI commands for managing GitHub Projects, Issues, and Pull Requests
license: MIT
compatibility: opencode
metadata:
  tool: github-cli
  category: project-management
  audience: developers
---

# GitHub Project Management with gh CLI

## Overview
This skill provides GitHub CLI (`gh`) commands for managing GitHub Projects, Issues, and Pull Requests within the agents hive workflow.

## Authentication
Ensure you're authenticated with GitHub CLI:
```bash
gh auth status
# If not authenticated: gh auth login
```

## Project Management Commands

### 1. List Projects
```bash
# List projects for current repository
gh project list --owner digit1024 --repo LunaAI

# List user projects
gh project list --user digit1024

# List organization projects
gh project list --org organization-name
```

### 2. View Project Details
```bash
# View project details by number
gh project view 2 --owner digit1024

# View with field details
gh project view 2 --owner digit1024 --format json
```

### 3. Manage Project Items

#### Add Issue to Project
```bash
# Add issue to project
gh project item-add 2 --owner digit1024 --url https://github.com/digit1024/LunaAI/issues/123

# Alternative using issue number
gh project item-add 2 --owner digit1024 --type ISSUE --id 123
```

#### List Project Items
```bash
# List items in project
gh project item-list 2 --owner digit1024

# List with details
gh project item-list 2 --owner digit1024 --format json
```

#### Move Item Between Columns
```bash
# First get item ID from item-list
gh project item-list 2 --owner digit1024 --format json | jq '.items[] | select(.content.number == 123) | .id'

# Then move to specific column (get column ID from project view)
gh project item-edit 2 --owner digit1024 --item-id ITEM_ID --field-id STATUS_FIELD --project-id 2 --single-select-option-id COLUMN_ID
```

### 4. Project Fields (Columns)

#### List Project Fields
```bash
# List all fields in project
gh project field-list 2 --owner digit1024

# Get specific field (like Status)
gh project field-list 2 --owner digit1024 --format json | jq '.fields[] | select(.name == "Status")'
```

#### Get Field Options (Column IDs)
```bash
# Get options for Status field
gh project field-list 2 --owner digit1024 --format json | jq '.fields[] | select(.name == "Status") | .options[]'
```

## Issue Management Commands

### 1. Create Issues
```bash
# Create issue with title
gh issue create --title "Feature: Add user authentication" --body "Implement JWT-based authentication system"

# Create from file
gh issue create --title "Bug: Fix login error" --body-file bug-description.md

# Assign labels
gh issue create --title "Enhancement: Improve performance" --body "Optimize database queries" --label enhancement,performance
```

### 2. View and List Issues
```bash
# List open issues
gh issue list

# List with specific state
gh issue list --state closed

# List with labels
gh issue list --label bug,high-priority

# View specific issue
gh issue view 123 --comments
```

### 3. Update Issues
```bash
# Edit issue
gh issue edit 123 --title "Updated title" --body "Updated description"

# Add labels
gh issue edit 123 --add-label bug,needs-review

# Remove labels
gh issue edit 123 --remove-label wontfix

# Assign to user
gh issue edit 123 --assignee @digit1024

# Set milestone
gh issue edit 123 --milestone "v1.0"
```

### 4. Issue Comments
```bash
# Add comment
gh issue comment 123 --body "ARCHITECT: Analysis complete. Moving to Ready."

# Edit comment
gh issue comment 123 --body "Updated comment" --id COMMENT_ID

# List comments
gh issue view 123 --comments
```

## Pull Request Management

### 1. Create PRs
```bash
# Create PR from current branch
gh pr create --title "Feature: Add authentication" --body "Implements JWT authentication"

# Create with specific base branch
gh pr create --base main --head feature-branch --title "Feature: XYZ" --body "Description"

# Create as draft
gh pr create --draft --title "WIP: Feature in progress"
```

### 2. View and List PRs
```bash
# List PRs
gh pr list

# View specific PR
gh pr view 45 --comments

# View PR diff
gh pr diff 45
```

### 3. Update PRs
```bash
# Edit PR
gh pr edit 45 --title "Updated title" --body "Updated description"

# Request review
gh pr edit 45 --add-reviewer @reviewer1,@reviewer2

# Add labels
gh pr edit 45 --add-label ready-for-review

# Merge PR
gh pr merge 45 --squash --delete-branch
```

### 4. PR Review
```bash
# View review comments
gh pr view 45 --comments

# Add review comment
gh pr review 45 --comment --body "REVIEWER: Looks good overall, minor suggestions."

# Approve PR
gh pr review 45 --approve --body "REVIEWER: Approved. Good work!"

# Request changes
gh pr review 45 --request-changes --body "REVIEWER: Please fix the security issues mentioned."
```

## Workflow Automation Scripts

### Move Issue to Specific Column
```bash
#!/bin/bash
# move_to_column.sh
# Usage: ./move_to_column.sh <issue_number> <column_name>

ISSUE_NUMBER=$1
COLUMN_NAME=$2
PROJECT_NUMBER=2
OWNER=digit1024
REPO=LunaAI

# Get item ID
ITEM_ID=$(gh project item-list $PROJECT_NUMBER --owner $OWNER --format json | \
  jq -r ".items[] | select(.content.number == $ISSUE_NUMBER) | .id")

# Get field ID for Status
FIELD_ID=$(gh project field-list $PROJECT_NUMBER --owner $OWNER --format json | \
  jq -r '.fields[] | select(.name == "Status") | .id')

# Get column option ID
COLUMN_ID=$(gh project field-list $PROJECT_NUMBER --owner $OWNER --format json | \
  jq -r ".fields[] | select(.name == \"Status\") | .options[] | select(.name == \"$COLUMN_NAME\") | .id")

# Move item
gh project item-edit $PROJECT_NUMBER --owner $OWNER --item-id $ITEM_ID \
  --field-id $FIELD_ID --project-id $PROJECT_NUMBER \
  --single-select-option-id $COLUMN_ID

echo "Moved issue #$ISSUE_NUMBER to '$COLUMN_NAME' column"
```

### Create Issue with Project Assignment
```bash
#!/bin/bash
# create_issue_with_project.sh
# Usage: ./create_issue_with_project.sh <title> <body> <column_name>

TITLE=$1
BODY=$2
COLUMN_NAME=$3
PROJECT_NUMBER=2
OWNER=digit1024
REPO=LunaAI

# Create issue
ISSUE_URL=$(gh issue create --title "$TITLE" --body "$BODY" --repo $OWNER/$REPO --json url --jq '.url')

# Add to project
gh project item-add $PROJECT_NUMBER --owner $OWNER --url $ISSUE_URL

echo "Created issue and added to project: $ISSUE_URL"
```

## Agent-Specific Workflows

### Architect Agent Workflow
```bash
# 1. Analyze issue from Backlog
gh issue view 123 --comments

# 2. Add architectural analysis comment
gh issue comment 123 --body "ARCHITECT: Analysis complete. Architecture designed. Moving to Ready."

# 3. Move to Ready column
./move_to_column.sh 123 "Ready"
```

### Coder Agent Workflow
```bash
# 1. Read issue from Ready column
gh issue view 123 --comments

# 2. Implement code changes
# ... coding work ...

# 3. Create PR
gh pr create --title "Implement feature from issue #123" --body "Closes #123"

# 4. Add PR link to issue
gh issue comment 123 --body "CODER: Implementation complete. PR created: $(gh pr view --json url --jq '.url')"

# 5. Move to In Review
./move_to_column.sh 123 "In Review"
```

### Reviewer Agent Workflow
```bash
# 1. Review PR from In Review column
gh pr view 45 --comments
gh pr diff 45

# 2. Add review comments
gh pr review 45 --comment --body "REVIEWER: Code review completed. Minor suggestions."

# 3. Approve or request changes
gh pr review 45 --approve --body "REVIEWER: Approved. Moving to QA."

# 4. Move to QA column
./move_to_column.sh 123 "QA"
```

### QA Agent Workflow
```bash
# 1. Test implementation from QA column
# ... run tests ...

# 2. Add test results comment
gh issue comment 123 --body "QA: All tests pass. Deployment verified."

# 3. Move to Done column
./move_to_column.sh 123 "Done"
```

## Error Handling

### Check Authentication
```bash
if ! gh auth status >/dev/null 2>&1; then
  echo "Error: Not authenticated with GitHub CLI"
  echo "Run: gh auth login"
  exit 1
fi
```

### Validate Project Access
```bash
if ! gh project view 2 --owner digit1024 >/dev/null 2>&1; then
  echo "Error: Cannot access project 2"
  echo "Check permissions or project number"
  exit 1
fi
```

### Handle Missing Dependencies
```bash
if ! command -v jq &> /dev/null; then
  echo "Error: jq is required but not installed"
  echo "Install with: sudo apt-get install jq"
  exit 1
fi
```

## Best Practices

1. **Always check authentication** before running commands
2. **Use JSON output with jq** for parsing responses
3. **Handle errors gracefully** in scripts
4. **Log actions** for audit trail
5. **Validate inputs** before executing commands
6. **Use dry-run mode** when testing new scripts
7. **Keep tokens secure** and never commit to version control

## References
- [GitHub CLI Documentation](https://cli.github.com/manual/)
- [GitHub Projects API](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects)
- [jq Manual](https://stedolan.github.io/jq/manual/)